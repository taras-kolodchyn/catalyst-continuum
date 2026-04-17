use std::{sync::OnceLock, time::Duration};

use anyhow::Context;
use opentelemetry::{
    KeyValue, global,
    metrics::{Counter, Histogram},
    trace::TracerProvider as _,
};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider, Temporality},
    trace::SdkTracerProvider,
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

const DEFAULT_LOG_FILTER: &str = "info,catalyst_continuum_orchestrator=debug";
const DEFAULT_METRIC_EXPORT_INTERVAL_SECONDS: u64 = 15;
const SERVICE_NAME: &str = "catalyst-continuum-orchestrator";
const SERVICE_NAMESPACE: &str = "catalyst-continuum";

#[derive(Default)]
pub struct Telemetry {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

struct Instruments {
    command_executions: Counter<u64>,
    command_duration_ms: Histogram<f64>,
    http_requests: Counter<u64>,
    http_request_duration_ms: Histogram<f64>,
    brief_submissions: Counter<u64>,
    brief_submission_duration_ms: Histogram<f64>,
    task_executions: Counter<u64>,
    task_execution_duration_ms: Histogram<f64>,
    worker_cycles: Counter<u64>,
    worker_cycle_duration_ms: Histogram<f64>,
}

static INSTRUMENTS: OnceLock<Instruments> = OnceLock::new();

pub fn init() -> anyhow::Result<Telemetry> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .compact()
        .with_writer(std::io::stderr);

    if !otlp_configured() {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
        return Ok(Telemetry::default());
    }

    let resource = resource();
    let tracer_provider = build_tracer_provider(resource.clone())?;
    let tracer = tracer_provider.tracer(SERVICE_NAME);
    let meter_provider = build_meter_provider(resource.clone())?;
    let logger_provider = build_logger_provider(resource)?;

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(OpenTelemetryTracingBridge::new(&logger_provider))
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();

    tracing::info!("OpenTelemetry OTLP export enabled");

    Ok(Telemetry {
        tracer_provider: Some(tracer_provider),
        meter_provider: Some(meter_provider),
        logger_provider: Some(logger_provider),
    })
}

impl Telemetry {
    pub fn shutdown(&mut self) {
        if let Some(logger_provider) = self.logger_provider.take()
            && let Err(error) = logger_provider.shutdown()
        {
            eprintln!("failed to shutdown OpenTelemetry log exporter: {error}");
        }

        if let Some(meter_provider) = self.meter_provider.take()
            && let Err(error) = meter_provider.shutdown()
        {
            eprintln!("failed to shutdown OpenTelemetry metric exporter: {error}");
        }

        if let Some(tracer_provider) = self.tracer_provider.take()
            && let Err(error) = tracer_provider.shutdown()
        {
            eprintln!("failed to shutdown OpenTelemetry trace exporter: {error}");
        }
    }
}

pub fn record_command_execution(command: &str, outcome: &str, duration: Duration) {
    let instruments = instruments();
    let attributes = [
        KeyValue::new("command", command.to_string()),
        KeyValue::new("outcome", outcome.to_string()),
    ];

    instruments.command_executions.add(1, &attributes);
    instruments
        .command_duration_ms
        .record(duration_ms(duration), &attributes);
}

pub fn record_http_request(method: &str, route: &str, status_code: u64, duration: Duration) {
    let instruments = instruments();
    let attributes = [
        KeyValue::new("http.method", method.to_string()),
        KeyValue::new("http.route", route.to_string()),
        KeyValue::new("http.status_code", status_code as i64),
    ];

    instruments.http_requests.add(1, &attributes);
    instruments
        .http_request_duration_ms
        .record(duration_ms(duration), &attributes);
}

pub fn record_brief_submission(
    trigger: &str,
    pack_id: &str,
    dry_run: bool,
    task_count: u64,
    artifact_count: u64,
    duration: Duration,
) {
    let instruments = instruments();
    let attributes = [
        KeyValue::new("trigger", trigger.to_string()),
        KeyValue::new("pack_id", pack_id.to_string()),
        KeyValue::new("dry_run", dry_run),
    ];

    instruments.brief_submissions.add(1, &attributes);
    instruments
        .brief_submission_duration_ms
        .record(duration_ms(duration), &attributes);

    tracing::info!(
        trigger,
        pack_id,
        dry_run,
        task_count,
        artifact_count,
        duration_ms = duration_ms(duration),
        "recorded brief submission telemetry"
    );
}

pub fn record_task_execution(provider: &str, status: &str, duration: Duration) {
    let instruments = instruments();
    let attributes = [
        KeyValue::new("provider", provider.to_string()),
        KeyValue::new("status", status.to_string()),
    ];

    instruments.task_executions.add(1, &attributes);
    instruments
        .task_execution_duration_ms
        .record(duration_ms(duration), &attributes);
}

pub fn record_worker_cycle(outcome: &str, once: bool, duration: Duration) {
    let instruments = instruments();
    let attributes = [
        KeyValue::new("outcome", outcome.to_string()),
        KeyValue::new("once", once),
    ];

    instruments.worker_cycles.add(1, &attributes);
    instruments
        .worker_cycle_duration_ms
        .record(duration_ms(duration), &attributes);
}

fn instruments() -> &'static Instruments {
    INSTRUMENTS.get_or_init(|| {
        let meter = global::meter(SERVICE_NAME);

        Instruments {
            command_executions: meter
                .u64_counter("catalyst_command_executions")
                .with_description("Count of orchestrator command executions")
                .build(),
            command_duration_ms: meter
                .f64_histogram("catalyst_command_duration_ms")
                .with_description("Duration of orchestrator command executions in milliseconds")
                .build(),
            http_requests: meter
                .u64_counter("catalyst_http_requests")
                .with_description("Count of orchestrator HTTP requests")
                .build(),
            http_request_duration_ms: meter
                .f64_histogram("catalyst_http_request_duration_ms")
                .with_description("Duration of orchestrator HTTP requests in milliseconds")
                .build(),
            brief_submissions: meter
                .u64_counter("catalyst_brief_submissions")
                .with_description("Count of accepted brief submissions")
                .build(),
            brief_submission_duration_ms: meter
                .f64_histogram("catalyst_brief_submission_duration_ms")
                .with_description("Duration of brief submissions in milliseconds")
                .build(),
            task_executions: meter
                .u64_counter("catalyst_task_executions")
                .with_description("Count of executed tasks")
                .build(),
            task_execution_duration_ms: meter
                .f64_histogram("catalyst_task_execution_duration_ms")
                .with_description("Duration of task execution cycles in milliseconds")
                .build(),
            worker_cycles: meter
                .u64_counter("catalyst_worker_cycles")
                .with_description("Count of worker loop cycles")
                .build(),
            worker_cycle_duration_ms: meter
                .f64_histogram("catalyst_worker_cycle_duration_ms")
                .with_description("Duration of worker loop cycles in milliseconds")
                .build(),
        }
    })
}

fn resource() -> Resource {
    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| SERVICE_NAME.into());
    let service_version = std::env::var("CATALYST_SERVICE_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());

    Resource::builder()
        .with_attributes([
            KeyValue::new("service.name", service_name),
            KeyValue::new("service.namespace", SERVICE_NAMESPACE),
            KeyValue::new("service.version", service_version),
            KeyValue::new("service.instance.id", std::process::id().to_string()),
        ])
        .build()
}

fn build_tracer_provider(resource: Resource) -> anyhow::Result<SdkTracerProvider> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .build()
        .context("failed to build OTLP trace exporter")?;

    Ok(SdkTracerProvider::builder()
        .with_resource(resource)
        .with_simple_exporter(exporter)
        .build())
}

fn build_meter_provider(resource: Resource) -> anyhow::Result<SdkMeterProvider> {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .with_temporality(Temporality::Cumulative)
        .build()
        .context("failed to build OTLP metric exporter")?;
    let reader = PeriodicReader::builder(exporter)
        .with_interval(metric_export_interval())
        .build();
    let meter_provider = SdkMeterProvider::builder()
        .with_resource(resource)
        .with_reader(reader)
        .build();

    global::set_meter_provider(meter_provider.clone());

    Ok(meter_provider)
}

fn build_logger_provider(resource: Resource) -> anyhow::Result<SdkLoggerProvider> {
    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .build()
        .context("failed to build OTLP log exporter")?;

    Ok(SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_simple_exporter(exporter)
        .build())
}

fn metric_export_interval() -> Duration {
    let seconds = std::env::var("CATALYST_OTEL_EXPORT_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_METRIC_EXPORT_INTERVAL_SECONDS);

    Duration::from_secs(seconds)
}

fn otlp_configured() -> bool {
    [
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

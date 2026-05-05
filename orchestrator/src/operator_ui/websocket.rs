use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::{
    io::Cursor,
    path::Path,
    thread,
    time::{Duration, Instant},
};
use tiny_http::{Header, Request, Response, StatusCode};
use tungstenite::{Message, protocol::Role, protocol::WebSocket};
use uuid::Uuid;

use crate::{
    config::InstanceConfigReport,
    models::{
        repository_signal::{RepositorySignalListFilters, RepositorySignalSummary},
        run::{RunDetail, RunSummary},
        run_event::RunEventSummary,
        webhook::{
            GitHubWebhookActionRequestListFilters, GitHubWebhookActionRequestSummary,
            GitHubWebhookDeliverySummary,
        },
    },
    storage::postgres::{PostgresRunStore, RunEventListFilters, RunListFilters},
};

use super::{OperatorUiDashboardSnapshotResponse, dashboard_snapshot};

const WEBSOCKET_ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const HELLO_HEARTBEAT_INTERVAL_MS: u64 = 15_000;
const RUN_LIST_LIMIT: usize = 12;
const RUN_EVENT_LIMIT: usize = 30;
const WEBHOOK_LIST_LIMIT: usize = 8;
const WEBHOOK_ACTION_REQUEST_LIST_LIMIT: usize = 8;
const REPOSITORY_SIGNAL_LIST_LIMIT: usize = 8;
const PENDING_AUTOMATION_STATUS: &str = "pending";
const DASHBOARD_PUBLISH_INTERVAL: Duration = Duration::from_secs(5);
const COLLECTION_PUBLISH_INTERVAL: Duration = Duration::from_millis(1_200);
const RUN_DETAIL_PUBLISH_INTERVAL: Duration = Duration::from_millis(900);
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(HELLO_HEARTBEAT_INTERVAL_MS);
const IDLE_SLEEP_INTERVAL: Duration = Duration::from_millis(250);
const BUSY_SLEEP_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub(crate) struct OperatorUiWebsocketWatch {
    pub run_id: Option<Uuid>,
    pub run_filters: RunListFilters,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OperatorUiRealtimeMessage {
    Hello {
        heartbeat_interval_ms: u64,
    },
    DashboardSnapshot {
        snapshot: Box<OperatorUiDashboardSnapshotResponse>,
    },
    RunsSnapshot {
        response: Box<OperatorUiRealtimeRunsResponse>,
    },
    AutomationSnapshot {
        response: Box<OperatorUiRealtimeAutomationResponse>,
    },
    RunDetailSnapshot {
        response: Box<OperatorUiRealtimeRunDetailResponse>,
    },
    SelectedRunMissing {
        run_id: Uuid,
        error: String,
    },
    Heartbeat,
}

#[derive(Debug, Serialize)]
pub(crate) struct OperatorUiRealtimeRunsResponse {
    pub count: usize,
    pub runs: Vec<RunSummary>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OperatorUiRealtimeRunDetailResponse {
    pub run: RunDetail,
    pub events: Vec<RunEventSummary>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OperatorUiRealtimeAutomationResponse {
    pub webhook_actions: OperatorUiRealtimeWebhookActionsResponse,
    pub repository_signals: OperatorUiRealtimeRepositorySignalsResponse,
    pub webhook_deliveries: OperatorUiRealtimeWebhookDeliveriesResponse,
}

#[derive(Debug, Serialize)]
pub(crate) struct OperatorUiRealtimeWebhookDeliveriesResponse {
    pub count: usize,
    pub deliveries: Vec<GitHubWebhookDeliverySummary>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OperatorUiRealtimeWebhookActionsResponse {
    pub count: usize,
    pub requests: Vec<GitHubWebhookActionRequestSummary>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OperatorUiRealtimeRepositorySignalsResponse {
    pub count: usize,
    pub signals: Vec<RepositorySignalSummary>,
}

#[derive(Debug, Default)]
struct LastPublishedPayloads {
    dashboard: Option<String>,
    runs: Option<String>,
    automation: Option<String>,
    run_detail: Option<String>,
    missing_run: Option<String>,
}

pub(crate) fn is_websocket_upgrade_request(request: &Request) -> bool {
    header_value(request.headers(), "Upgrade")
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

pub(crate) fn handle_websocket_request(
    request: Request,
    query: Option<&str>,
    database_url: &str,
    instance_config: &InstanceConfigReport,
    artifact_root: &Path,
) -> Result<u16> {
    if !is_websocket_upgrade_request(&request) {
        request
            .respond(websocket_error_response(
                StatusCode(426),
                "operator UI live updates require a websocket upgrade request",
            ))
            .context("failed to send websocket upgrade requirement response")?;
        return Ok(StatusCode(426).0);
    }

    let Some(websocket_key) = header_value(request.headers(), "Sec-WebSocket-Key") else {
        request
            .respond(websocket_error_response(
                StatusCode(400),
                "missing Sec-WebSocket-Key header",
            ))
            .context("failed to send websocket header validation response")?;
        return Ok(StatusCode(400).0);
    };

    let watch = match parse_watch_query(query) {
        Ok(watch) => watch,
        Err(error) => {
            request
                .respond(websocket_error_response(
                    StatusCode(400),
                    &format!("invalid operator UI websocket watch: {error}"),
                ))
                .context("failed to send websocket watch validation response")?;
            return Ok(StatusCode(400).0);
        }
    };
    let accept_value = websocket_accept_value(websocket_key);
    let switching_protocols = Response::new_empty(StatusCode(101))
        .with_header(header("Upgrade", "websocket"))
        .with_header(header("Connection", "Upgrade"))
        .with_header(header("Sec-WebSocket-Accept", accept_value));

    let stream = request.upgrade("websocket", switching_protocols);
    let database_url = database_url.to_string();
    let instance_config = instance_config.clone();
    let artifact_root = artifact_root.to_path_buf();
    thread::spawn(move || {
        if let Err(error) = run_websocket_session(
            stream,
            watch,
            &database_url,
            &instance_config,
            &artifact_root,
        ) {
            tracing::warn!(error = %error, "operator UI websocket session terminated");
        }
    });

    Ok(StatusCode(101).0)
}

fn run_websocket_session(
    stream: Box<dyn tiny_http::ReadWrite + Send>,
    watch: OperatorUiWebsocketWatch,
    database_url: &str,
    instance_config: &InstanceConfigReport,
    artifact_root: &Path,
) -> Result<()> {
    let mut websocket = WebSocket::from_raw_socket(stream, Role::Server, None);
    let mut store = PostgresRunStore::connect(database_url)?;
    let mut last = LastPublishedPayloads::default();
    let mut next_dashboard_publish = Instant::now();
    let mut next_collection_publish = Instant::now();
    let mut next_run_detail_publish = Instant::now();
    let mut next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;

    send_message(
        &mut websocket,
        &OperatorUiRealtimeMessage::Hello {
            heartbeat_interval_ms: HELLO_HEARTBEAT_INTERVAL_MS,
        },
    )?;

    loop {
        let now = Instant::now();
        let mut sent = false;

        if now >= next_dashboard_publish {
            let snapshot =
                dashboard_snapshot(store.probe_readiness(), instance_config, artifact_root);
            sent |= maybe_send_snapshot(
                &mut websocket,
                &mut last.dashboard,
                &OperatorUiRealtimeMessage::DashboardSnapshot {
                    snapshot: Box::new(snapshot),
                },
            )?;
            next_dashboard_publish = now + DASHBOARD_PUBLISH_INTERVAL;
        }

        if now >= next_collection_publish {
            let runs = live_runs_response(&mut store, &watch.run_filters)?;
            sent |= maybe_send_snapshot(
                &mut websocket,
                &mut last.runs,
                &OperatorUiRealtimeMessage::RunsSnapshot {
                    response: Box::new(runs),
                },
            )?;

            let automation = live_automation_response(&mut store)?;
            sent |= maybe_send_snapshot(
                &mut websocket,
                &mut last.automation,
                &OperatorUiRealtimeMessage::AutomationSnapshot {
                    response: Box::new(automation),
                },
            )?;

            next_collection_publish = now + COLLECTION_PUBLISH_INTERVAL;
        }

        if now >= next_run_detail_publish {
            if let Some(run_id) = watch.run_id {
                match live_run_detail_response(&mut store, run_id)? {
                    Some(response) => {
                        last.missing_run = None;
                        sent |= maybe_send_snapshot(
                            &mut websocket,
                            &mut last.run_detail,
                            &OperatorUiRealtimeMessage::RunDetailSnapshot {
                                response: Box::new(response),
                            },
                        )?;
                    }
                    None => {
                        last.run_detail = None;
                        sent |= maybe_send_snapshot(
                            &mut websocket,
                            &mut last.missing_run,
                            &OperatorUiRealtimeMessage::SelectedRunMissing {
                                run_id,
                                error: format!("run {run_id} is no longer available"),
                            },
                        )?;
                    }
                }
            } else {
                last.run_detail = None;
                last.missing_run = None;
            }

            next_run_detail_publish = now + RUN_DETAIL_PUBLISH_INTERVAL;
        }

        if now >= next_heartbeat {
            send_message(&mut websocket, &OperatorUiRealtimeMessage::Heartbeat)?;
            next_heartbeat = now + HEARTBEAT_INTERVAL;
            sent = true;
        }

        thread::sleep(if sent {
            BUSY_SLEEP_INTERVAL
        } else {
            IDLE_SLEEP_INTERVAL
        });
    }
}

fn live_runs_response(
    store: &mut PostgresRunStore,
    filters: &RunListFilters,
) -> Result<OperatorUiRealtimeRunsResponse> {
    let runs = store.list_runs_filtered(RUN_LIST_LIMIT, filters)?;
    Ok(OperatorUiRealtimeRunsResponse {
        count: runs.len(),
        runs,
    })
}

fn pending_webhook_action_filters() -> GitHubWebhookActionRequestListFilters {
    GitHubWebhookActionRequestListFilters {
        status: Some(PENDING_AUTOMATION_STATUS.to_string()),
        action: None,
    }
}

fn pending_repository_signal_filters() -> RepositorySignalListFilters {
    RepositorySignalListFilters {
        status: Some(PENDING_AUTOMATION_STATUS.to_string()),
        signal_kind: None,
        repository_full_name: None,
    }
}

fn live_automation_response(
    store: &mut PostgresRunStore,
) -> Result<OperatorUiRealtimeAutomationResponse> {
    let webhook_actions = store.list_github_webhook_action_requests(
        WEBHOOK_ACTION_REQUEST_LIST_LIMIT,
        &pending_webhook_action_filters(),
    )?;
    let repository_signals = store.list_repository_signals(
        REPOSITORY_SIGNAL_LIST_LIMIT,
        &pending_repository_signal_filters(),
    )?;
    let webhook_deliveries =
        store.list_github_webhook_deliveries(WEBHOOK_LIST_LIMIT, &Default::default())?;

    Ok(OperatorUiRealtimeAutomationResponse {
        webhook_actions: OperatorUiRealtimeWebhookActionsResponse {
            count: webhook_actions.len(),
            requests: webhook_actions,
        },
        repository_signals: OperatorUiRealtimeRepositorySignalsResponse {
            count: repository_signals.len(),
            signals: repository_signals,
        },
        webhook_deliveries: OperatorUiRealtimeWebhookDeliveriesResponse {
            count: webhook_deliveries.len(),
            deliveries: webhook_deliveries,
        },
    })
}

fn live_run_detail_response(
    store: &mut PostgresRunStore,
    run_id: Uuid,
) -> Result<Option<OperatorUiRealtimeRunDetailResponse>> {
    let Some(run) = store.fetch_run_detail(run_id)? else {
        return Ok(None);
    };
    let events = store.list_run_events(run_id, RUN_EVENT_LIMIT, &RunEventListFilters::default())?;

    Ok(Some(OperatorUiRealtimeRunDetailResponse { run, events }))
}

fn maybe_send_snapshot(
    websocket: &mut WebSocket<Box<dyn tiny_http::ReadWrite + Send>>,
    last_serialized: &mut Option<String>,
    message: &OperatorUiRealtimeMessage,
) -> Result<bool> {
    let serialized =
        serde_json::to_string(message).context("failed to serialize websocket message")?;
    if last_serialized.as_ref() == Some(&serialized) {
        return Ok(false);
    }

    websocket
        .send(Message::Text(serialized.clone()))
        .context("failed to write websocket snapshot")?;
    *last_serialized = Some(serialized);
    Ok(true)
}

fn send_message(
    websocket: &mut WebSocket<Box<dyn tiny_http::ReadWrite + Send>>,
    message: &OperatorUiRealtimeMessage,
) -> Result<()> {
    let serialized =
        serde_json::to_string(message).context("failed to serialize websocket message")?;
    websocket
        .send(Message::Text(serialized))
        .context("failed to write websocket message")
}

fn parse_watch_query(query: Option<&str>) -> Result<OperatorUiWebsocketWatch> {
    let mut run_id = None;
    let mut status = None;

    let Some(query) = query else {
        return Ok(OperatorUiWebsocketWatch {
            run_id,
            run_filters: RunListFilters::default(),
        });
    };

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }

        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key.is_empty() {
            continue;
        }

        let trimmed_value = value.trim();
        match key {
            "run" if !trimmed_value.is_empty() => {
                run_id = Some(Uuid::parse_str(trimmed_value).with_context(|| {
                    format!("invalid operator UI websocket run `{trimmed_value}`")
                })?);
            }
            "status" if !trimmed_value.is_empty() => {
                status = Some(trimmed_value.to_string());
            }
            _ => {}
        }
    }

    Ok(OperatorUiWebsocketWatch {
        run_id,
        run_filters: RunListFilters::from_inputs(status.as_deref(), None)?,
    })
}

fn websocket_accept_value(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(WEBSOCKET_ACCEPT_GUID.as_bytes());
    STANDARD.encode(hasher.finalize())
}

fn websocket_error_response(status: StatusCode, message: &str) -> Response<Cursor<Vec<u8>>> {
    Response::from_string(message)
        .with_status_code(status)
        .with_header(header("Content-Type", "text/plain; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"))
}

fn header_value<'a>(headers: &'a [Header], name: &'static str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
}

fn header(name: &str, value: impl AsRef<str>) -> Header {
    Header::from_bytes(name, value.as_ref())
        .unwrap_or_else(|_| panic!("invalid websocket response header: {name}"))
}

#[cfg(test)]
mod tests {
    use super::{
        parse_watch_query, pending_repository_signal_filters, pending_webhook_action_filters,
        websocket_accept_value,
    };
    use uuid::Uuid;

    #[test]
    fn parses_empty_watch_query() {
        let watch = parse_watch_query(None).expect("empty watch query should parse");

        assert_eq!(watch.run_id, None);
        assert_eq!(watch.run_filters.status, None);
        assert_eq!(watch.run_filters.target_pack, None);
    }

    #[test]
    fn parses_selected_run_and_status_filter() {
        let run_id = Uuid::new_v4();
        let watch = parse_watch_query(Some(&format!("run={run_id}&status=queued")))
            .expect("watch query should parse");

        assert_eq!(watch.run_id, Some(run_id));
        assert_eq!(watch.run_filters.status.as_deref(), Some("queued"));
        assert_eq!(watch.run_filters.target_pack, None);
    }

    #[test]
    fn parses_noisy_blank_watch_query_as_default_filters() {
        let watch = parse_watch_query(Some("&&unknown=value&run=&status=&=ignored&orphan"))
            .expect("noisy watch query should parse");

        assert_eq!(watch.run_id, None);
        assert_eq!(watch.run_filters.status, None);
        assert_eq!(watch.run_filters.target_pack, None);
    }

    #[test]
    fn parses_trimmed_watch_query_and_normalizes_status_alias() {
        let run_id = Uuid::new_v4();
        let watch = parse_watch_query(Some(&format!("run= {run_id} &status= running ")))
            .expect("watch query should parse");

        assert_eq!(watch.run_id, Some(run_id));
        assert_eq!(watch.run_filters.status.as_deref(), Some("executing"));
        assert_eq!(watch.run_filters.target_pack, None);
    }

    #[test]
    fn rejects_invalid_selected_run() {
        let error =
            parse_watch_query(Some("run=not-a-uuid")).expect_err("invalid run id should fail");

        assert!(
            error
                .to_string()
                .contains("invalid operator UI websocket run")
        );
    }

    #[test]
    fn rejects_invalid_status_filter() {
        let error = parse_watch_query(Some("status=unknown"))
            .expect_err("invalid status filter should fail");

        assert!(error.to_string().contains("invalid run status filter"));
    }

    #[test]
    fn derives_expected_websocket_accept_value() {
        assert_eq!(
            websocket_accept_value("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn automation_snapshots_only_track_pending_follow_up_items() {
        let webhook_filters = pending_webhook_action_filters();
        let signal_filters = pending_repository_signal_filters();

        assert_eq!(webhook_filters.status.as_deref(), Some("pending"));
        assert_eq!(webhook_filters.action, None);
        assert_eq!(signal_filters.status.as_deref(), Some("pending"));
        assert_eq!(signal_filters.signal_kind, None);
        assert_eq!(signal_filters.repository_full_name, None);
    }
}

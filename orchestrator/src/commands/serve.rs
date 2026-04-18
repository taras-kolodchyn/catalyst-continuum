use anyhow::Context;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::HashMap, time::Instant};
use tiny_http::{Header, Response, Server, StatusCode};
use uuid::Uuid;

use crate::{
    cli::ServeArgs,
    commands::{
        create_draft_pr, describe_artifact, describe_latest_artifact, evaluate_run_policy,
        evaluate_run_quality, export_pr_candidate, publish_pr_export, run_next_task,
        submit_brief::submit_validated_brief, worker,
    },
    planning::{
        brief_validation::validate_brief_document, pack_catalog::build_pack_catalog,
        packs::PackDefinition, pr_candidate,
    },
    runtime::RuntimeRegistry,
    storage::postgres::{DatabaseReadiness, PostgresRunStore, RunListFilters},
    telemetry,
};

const DEFAULT_RUN_LIST_LIMIT: usize = 20;

pub fn execute(args: ServeArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let runtime_registry = RuntimeRegistry::default();

    let server = Server::http(&args.bind_addr).map_err(|error| {
        anyhow::anyhow!(
            "failed to bind orchestrator scaffold to {}: {error}",
            args.bind_addr
        )
    })?;

    tracing::info!(
        bind_addr = %args.bind_addr,
        artifact_root = %args.artifact_root.display(),
        "orchestrator HTTP scaffold listening"
    );

    for mut request in server.incoming_requests() {
        let request_target = request.url().to_string();
        let (path, query) = split_request_target(&request_target);
        let method = request.method().as_str().to_string();
        let route = route_label(method.as_str(), path);
        let request_started_at = Instant::now();
        let request_span = tracing::info_span!("http.request", method = %method, route = route);
        let _request_span_guard = request_span.enter();

        let response = match (method.as_str(), path) {
            ("GET", "/") => json_response(
                StatusCode(200),
                &ServiceInfo {
                    service: "catalyst-continuum-orchestrator",
                    status: "ok",
                    endpoints: vec![
                        "/",
                        "/livez",
                        "/healthz",
                        "/readyz",
                        "/packs",
                        "/packs/{pack_id}",
                        "/artifacts/{artifact_id}",
                        "/runs",
                        "/runs/{run_id}",
                        "/runs/{run_id}/artifacts/latest/{artifact_type}",
                        "POST /runs/{run_id}/tasks/next",
                        "POST /runs/{run_id}/worker/once",
                        "POST /runs/{run_id}/evaluate-policy",
                        "POST /runs/{run_id}/evaluate-quality",
                        "POST /runs/{run_id}/export-pr-candidate",
                        "POST /runs/{run_id}/publish-pr-export",
                        "POST /runs/{run_id}/draft-pr",
                        "POST /briefs/validate",
                        "POST /briefs/submit",
                    ],
                },
            ),
            ("GET", "/livez") => json_response(
                StatusCode(200),
                &LivenessResponse {
                    status: "ok",
                    service: "catalyst-continuum-orchestrator",
                    database: "not_checked",
                },
            ),
            ("GET", "/healthz") | ("GET", "/readyz") => {
                let (status, payload) = readiness_payload(store.probe_readiness());
                json_response(status, &payload)
            }
            ("GET", "/packs") => match build_pack_catalog() {
                Ok(catalog) => json_response(StatusCode(200), &catalog),
                Err(error) => json_response(
                    StatusCode(500),
                    &ErrorResponse {
                        error: format!("failed to build pack catalog: {error}"),
                    },
                ),
            },
            ("GET", _) if single_path_segment(path, "/artifacts/").is_some() => {
                let artifact_id = single_path_segment(path, "/artifacts/")
                    .expect("artifact path guard should provide a single path segment");
                match parse_artifact_id(artifact_id) {
                    Ok(artifact_id) => {
                        match describe_artifact::describe_artifact(&mut store, artifact_id) {
                            Ok(Some(artifact)) => json_response(StatusCode(200), &artifact),
                            Ok(None) => json_response(
                                StatusCode(404),
                                &ErrorResponse {
                                    error: format!("artifact not found: {artifact_id}"),
                                },
                            ),
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to fetch artifact {artifact_id}: {error}"
                                    ),
                                },
                            ),
                        }
                    }
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            ("GET", "/runs") => match parse_list_runs_request(query) {
                Ok(list_request) => {
                    match store.list_runs_filtered(list_request.limit, &list_request.filters) {
                        Ok(runs) => json_response(
                            StatusCode(200),
                            &RecentRunsResponse {
                                count: runs.len(),
                                runs,
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to list runs: {error}"),
                            },
                        ),
                    }
                }
                Err(error) => json_response(
                    StatusCode(400),
                    &ErrorResponse {
                        error: error.to_string(),
                    },
                ),
            },
            ("GET", _) if latest_artifact_path_parts(path).is_some() => {
                match parse_latest_artifact_path(path) {
                    Ok((run_id, artifact_type)) => match store.fetch_run_summary(run_id) {
                        Ok(Some(_)) => {
                            match describe_latest_artifact::describe_latest_artifact(
                                &mut store,
                                run_id,
                                artifact_type,
                            ) {
                                Ok(Some(artifact)) => json_response(StatusCode(200), &artifact),
                                Ok(None) => json_response(
                                    StatusCode(404),
                                    &ErrorResponse {
                                        error: format!(
                                            "run {} does not have a latest artifact of type {}",
                                            run_id, artifact_type
                                        ),
                                    },
                                ),
                                Err(error) => json_response(
                                    StatusCode(500),
                                    &ErrorResponse {
                                        error: format!(
                                            "failed to fetch latest artifact {} for run {}: {error}",
                                            artifact_type, run_id
                                        ),
                                    },
                                ),
                            }
                        }
                        Ok(None) => json_response(
                            StatusCode(404),
                            &ErrorResponse {
                                error: format!("run not found: {run_id}"),
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to load run {run_id}: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            ("GET", _) if single_path_segment(path, "/runs/").is_some() => {
                let run_id = single_path_segment(path, "/runs/")
                    .expect("run path guard should provide a single path segment");
                match parse_run_id(run_id) {
                    Ok(run_id) => match store.fetch_run_detail(run_id) {
                        Ok(Some(run)) => json_response(StatusCode(200), &run),
                        Ok(None) => json_response(
                            StatusCode(404),
                            &ErrorResponse {
                                error: format!("run not found: {run_id}"),
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to fetch run {run_id}: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            ("GET", _) if single_path_segment(path, "/packs/").is_some() => {
                let pack_id = single_path_segment(path, "/packs/")
                    .expect("pack path guard should provide a single path segment");
                match PackDefinition::load_optional(pack_id) {
                    Ok(Some(pack)) => json_response(StatusCode(200), &pack),
                    Ok(None) => json_response(
                        StatusCode(404),
                        &ErrorResponse {
                            error: format!("pack not found: {pack_id}"),
                        },
                    ),
                    Err(error) => json_response(
                        StatusCode(500),
                        &ErrorResponse {
                            error: format!("failed to load pack {pack_id}: {error}"),
                        },
                    ),
                }
            }
            ("POST", "/briefs/validate") => match read_request_body(&mut request) {
                Ok(body) => match validate_brief_document(&body, "http:POST /briefs/validate") {
                    Ok(validated) => json_response(StatusCode(200), &validated.report),
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                },
                Err(error) => json_response(
                    StatusCode(400),
                    &ErrorResponse {
                        error: format!("failed to read request body: {error}"),
                    },
                ),
            },
            ("POST", "/briefs/submit") => match read_request_body(&mut request) {
                Ok(body) => match validate_brief_document(&body, "http:POST /briefs/submit") {
                    Ok(validated) => match submit_validated_brief(
                        validated,
                        "http:POST /briefs/submit",
                        Some(&args.database_url),
                        &args.artifact_root,
                        false,
                        "http",
                    ) {
                        Ok(submission) => json_response_with_headers(
                            StatusCode(201),
                            &submission,
                            &[header("Location", format!("/runs/{}", submission.run_id))],
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to submit brief: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                },
                Err(error) => json_response(
                    StatusCode(400),
                    &ErrorResponse {
                        error: format!("failed to read request body: {error}"),
                    },
                ),
            },
            ("POST", _) if path.starts_with("/runs/") && path.ends_with("/tasks/next") => {
                match parse_run_action_path(path, "/tasks/next") {
                    Ok(run_id) => match store.fetch_run_summary(run_id) {
                        Ok(Some(_)) => match run_next_task::execute_next_task(
                            &mut store,
                            &runtime_registry,
                            Some(run_id),
                            &args.artifact_root,
                        ) {
                            Ok(outcome) => json_response(StatusCode(200), &outcome),
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to execute next task for run {run_id}: {error}"
                                    ),
                                },
                            ),
                        },
                        Ok(None) => json_response(
                            StatusCode(404),
                            &ErrorResponse {
                                error: format!("run not found: {run_id}"),
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to load run {run_id}: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            ("POST", _) if path.starts_with("/runs/") && path.ends_with("/worker/once") => {
                match parse_run_action_path(path, "/worker/once") {
                    Ok(run_id) => match store.fetch_run_summary(run_id) {
                        Ok(Some(_)) => match worker::run_worker(
                            &mut store,
                            &runtime_registry,
                            Some(run_id),
                            &args.artifact_root,
                            true,
                            0,
                        ) {
                            Ok(report) => json_response(StatusCode(200), &report),
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to run worker once for run {run_id}: {error}"
                                    ),
                                },
                            ),
                        },
                        Ok(None) => json_response(
                            StatusCode(404),
                            &ErrorResponse {
                                error: format!("run not found: {run_id}"),
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to load run {run_id}: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            ("POST", _) if path.starts_with("/runs/") && path.ends_with("/evaluate-policy") => {
                match parse_run_action_path(path, "/evaluate-policy") {
                    Ok(run_id) => match store.fetch_run_summary(run_id) {
                        Ok(Some(_)) => match evaluate_run_policy::evaluate_run_policy(
                            &mut store,
                            run_id,
                            &args.artifact_root,
                        ) {
                            Ok(report) => json_response(StatusCode(200), &report),
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to evaluate run policy for run {run_id}: {error}"
                                    ),
                                },
                            ),
                        },
                        Ok(None) => json_response(
                            StatusCode(404),
                            &ErrorResponse {
                                error: format!("run not found: {run_id}"),
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to load run {run_id}: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            ("POST", _) if path.starts_with("/runs/") && path.ends_with("/evaluate-quality") => {
                match parse_run_action_path(path, "/evaluate-quality") {
                    Ok(run_id) => match store.fetch_run_summary(run_id) {
                        Ok(Some(_)) => match evaluate_run_quality::evaluate_run_quality(
                            &mut store,
                            run_id,
                            &args.artifact_root,
                        ) {
                            Ok(report) => json_response(StatusCode(200), &report),
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to evaluate quality gate for run {run_id}: {error}"
                                    ),
                                },
                            ),
                        },
                        Ok(None) => json_response(
                            StatusCode(404),
                            &ErrorResponse {
                                error: format!("run not found: {run_id}"),
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to load run {run_id}: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            ("POST", _) if path.starts_with("/runs/") && path.ends_with("/export-pr-candidate") => {
                match parse_run_action_path(path, "/export-pr-candidate") {
                    Ok(run_id) => match store.fetch_run_summary(run_id) {
                        Ok(Some(_)) => match store.find_latest_run_artifact(
                            run_id,
                            pr_candidate::PR_CANDIDATE_ARTIFACT_TYPE,
                        ) {
                            Ok(Some(_)) => {
                                match read_optional_json_body::<ExportPrCandidateRequest>(
                                    &mut request,
                                ) {
                                    Ok(payload) => match export_pr_candidate::export_pr_candidate(
                                        &mut store,
                                        run_id,
                                        &args.artifact_root,
                                        non_empty_option(payload.branch_name.as_deref()),
                                    ) {
                                        Ok(report) => json_response(StatusCode(200), &report),
                                        Err(error) => json_response(
                                            StatusCode(500),
                                            &ErrorResponse {
                                                error: format!(
                                                    "failed to export PR candidate for run {run_id}: {error}"
                                                ),
                                            },
                                        ),
                                    },
                                    Err(error) => json_response(
                                        StatusCode(400),
                                        &ErrorResponse {
                                            error: error.to_string(),
                                        },
                                    ),
                                }
                            }
                            Ok(None) => json_response(
                                StatusCode(409),
                                &ErrorResponse {
                                    error: format!(
                                        "PR export requires a pr_candidate artifact for run {run_id}"
                                    ),
                                },
                            ),
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to inspect PR candidate artifact for run {run_id}: {error}"
                                    ),
                                },
                            ),
                        },
                        Ok(None) => json_response(
                            StatusCode(404),
                            &ErrorResponse {
                                error: format!("run not found: {run_id}"),
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to load run {run_id}: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            ("POST", _) if path.starts_with("/runs/") && path.ends_with("/publish-pr-export") => {
                match parse_run_action_path(path, "/publish-pr-export") {
                    Ok(run_id) => match store.fetch_run_summary(run_id) {
                        Ok(Some(_)) => match store.find_latest_run_artifact(
                            run_id,
                            pr_candidate::PR_CANDIDATE_ARTIFACT_TYPE,
                        ) {
                            Ok(Some(_)) => match read_optional_json_body::<PublishPrExportRequest>(
                                &mut request,
                            ) {
                                Ok(payload) => match publish_pr_export::publish_pr_export(
                                    &mut store,
                                    run_id,
                                    &args.artifact_root,
                                    non_empty_option(payload.remote_url.as_deref()),
                                    payload.push,
                                ) {
                                    Ok(report) => json_response(StatusCode(200), &report),
                                    Err(error) => json_response(
                                        StatusCode(500),
                                        &ErrorResponse {
                                            error: format!(
                                                "failed to publish PR export for run {run_id}: {error}"
                                            ),
                                        },
                                    ),
                                },
                                Err(error) => json_response(
                                    StatusCode(400),
                                    &ErrorResponse {
                                        error: error.to_string(),
                                    },
                                ),
                            },
                            Ok(None) => json_response(
                                StatusCode(409),
                                &ErrorResponse {
                                    error: format!(
                                        "PR publication requires a pr_candidate artifact for run {run_id}; export a PR candidate first"
                                    ),
                                },
                            ),
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to inspect PR candidate artifact for run {run_id}: {error}"
                                    ),
                                },
                            ),
                        },
                        Ok(None) => json_response(
                            StatusCode(404),
                            &ErrorResponse {
                                error: format!("run not found: {run_id}"),
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to load run {run_id}: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            ("POST", _) if path.starts_with("/runs/") && path.ends_with("/draft-pr") => {
                match parse_run_action_path(path, "/draft-pr") {
                    Ok(run_id) => match store.fetch_run_summary(run_id) {
                        Ok(Some(_)) => match store.refresh_run_status(run_id) {
                            Ok(run_status) if run_status != "succeeded" => json_response(
                                StatusCode(409),
                                &ErrorResponse {
                                    error: format!(
                                        "draft PR creation requires a succeeded run, current status is {} for run {}",
                                        run_status, run_id
                                    ),
                                },
                            ),
                            Ok(_) => match store.find_latest_run_artifact(
                                run_id,
                                pr_candidate::PR_CANDIDATE_ARTIFACT_TYPE,
                            ) {
                                Ok(Some(_)) => {
                                    match read_optional_json_body::<CreateDraftPrRequest>(
                                        &mut request,
                                    ) {
                                        Ok(payload) => match create_draft_pr::create_draft_pr(
                                            &mut store,
                                            run_id,
                                            &args.artifact_root,
                                            non_empty_option(payload.remote_url.as_deref()),
                                            non_empty_option(payload.branch_name.as_deref()),
                                        ) {
                                            Ok(report) => json_response(StatusCode(200), &report),
                                            Err(error) => json_response(
                                                StatusCode(500),
                                                &ErrorResponse {
                                                    error: format!(
                                                        "failed to create draft PR for run {run_id}: {error}"
                                                    ),
                                                },
                                            ),
                                        },
                                        Err(error) => json_response(
                                            StatusCode(400),
                                            &ErrorResponse {
                                                error: error.to_string(),
                                            },
                                        ),
                                    }
                                }
                                Ok(None) => json_response(
                                    StatusCode(409),
                                    &ErrorResponse {
                                        error: format!(
                                            "draft PR creation requires a pr_candidate artifact for run {run_id}"
                                        ),
                                    },
                                ),
                                Err(error) => json_response(
                                    StatusCode(500),
                                    &ErrorResponse {
                                        error: format!(
                                            "failed to inspect PR candidate artifact for run {run_id}: {error}"
                                        ),
                                    },
                                ),
                            },
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to refresh run status for {run_id}: {error}"
                                    ),
                                },
                            ),
                        },
                        Ok(None) => json_response(
                            StatusCode(404),
                            &ErrorResponse {
                                error: format!("run not found: {run_id}"),
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to load run {run_id}: {error}"),
                            },
                        ),
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
            _ => json_response(
                StatusCode(404),
                &ErrorResponse {
                    error: format!("route not found: {} {}", method, path),
                },
            ),
        };
        let status_code = response.status_code().0;
        telemetry::record_http_request(
            method.as_str(),
            route,
            status_code.into(),
            request_started_at.elapsed(),
        );

        if let Err(error) = request.respond(response) {
            tracing::warn!(%error, "failed to send HTTP response");
        }
    }

    Ok(())
}

fn parse_run_id(value: &str) -> anyhow::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| anyhow::anyhow!("invalid run id `{value}`: {error}"))
}

fn parse_artifact_id(value: &str) -> anyhow::Result<Uuid> {
    Uuid::parse_str(value)
        .map_err(|error| anyhow::anyhow!("invalid artifact id `{value}`: {error}"))
}

fn parse_run_action_path(path: &str, suffix: &str) -> anyhow::Result<Uuid> {
    let run_id = path
        .strip_prefix("/runs/")
        .and_then(|value| value.strip_suffix(suffix))
        .ok_or_else(|| anyhow::anyhow!("invalid run action path: {path}"))?;

    parse_run_id(run_id)
}

fn latest_artifact_path_parts(path: &str) -> Option<(&str, &str)> {
    let remainder = path.strip_prefix("/runs/")?;
    let (run_id, artifact_type) = remainder.split_once("/artifacts/latest/")?;
    if run_id.is_empty() || artifact_type.is_empty() || artifact_type.contains('/') {
        return None;
    }

    Some((run_id, artifact_type))
}

fn parse_latest_artifact_path(path: &str) -> anyhow::Result<(Uuid, &str)> {
    let (run_id, artifact_type) = latest_artifact_path_parts(path)
        .ok_or_else(|| anyhow::anyhow!("invalid latest artifact path: {path}"))?;

    Ok((parse_run_id(run_id)?, artifact_type))
}

fn single_path_segment<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)
        .filter(|value| !value.is_empty() && !value.contains('/'))
}

fn split_request_target(request_target: &str) -> (&str, Option<&str>) {
    match request_target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (request_target, None),
    }
}

fn read_request_body(request: &mut tiny_http::Request) -> std::io::Result<String> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    Ok(body)
}

fn read_optional_json_body<T>(request: &mut tiny_http::Request) -> anyhow::Result<T>
where
    T: DeserializeOwned + Default,
{
    let body = read_request_body(request).context("failed to read request body")?;
    if body.trim().is_empty() {
        return Ok(T::default());
    }

    serde_json::from_str(&body).context("failed to parse JSON request body")
}

fn non_empty_option(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn parse_list_runs_request(query: Option<&str>) -> anyhow::Result<ListRunsRequest> {
    let query_pairs = parse_query_pairs(query);
    let limit = match query_pairs.get("limit") {
        Some(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<usize>()
            .map_err(|error| anyhow::anyhow!("invalid runs.limit `{}`: {error}", value.trim()))?,
        _ => DEFAULT_RUN_LIST_LIMIT,
    };
    let filters = RunListFilters::from_inputs(
        query_pairs.get("status").map(String::as_str),
        query_pairs.get("target_pack").map(String::as_str),
    )?;

    Ok(ListRunsRequest { limit, filters })
}

fn parse_query_pairs(query: Option<&str>) -> HashMap<String, String> {
    let mut pairs = HashMap::new();

    let Some(query) = query else {
        return pairs;
    };

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }

        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key.is_empty() {
            continue;
        }

        pairs.insert(key.to_string(), value.to_string());
    }

    pairs
}

fn readiness_payload(probe: anyhow::Result<DatabaseReadiness>) -> (StatusCode, ReadinessResponse) {
    match probe {
        Ok(readiness) if readiness.schema_ready => (
            StatusCode(200),
            ReadinessResponse {
                status: "ok",
                service: "catalyst-continuum-orchestrator",
                database: "ready",
                schema: "ready",
                database_name: Some(readiness.database_name),
                missing_tables: Vec::new(),
                error: None,
            },
        ),
        Ok(readiness) => (
            StatusCode(503),
            ReadinessResponse {
                status: "degraded",
                service: "catalyst-continuum-orchestrator",
                database: "ready",
                schema: "missing_tables",
                database_name: Some(readiness.database_name),
                missing_tables: readiness.missing_tables,
                error: None,
            },
        ),
        Err(error) => (
            StatusCode(503),
            ReadinessResponse {
                status: "error",
                service: "catalyst-continuum-orchestrator",
                database: "unavailable",
                schema: "unknown",
                database_name: None,
                missing_tables: Vec::new(),
                error: Some(error.to_string()),
            },
        ),
    }
}

fn route_label(method: &str, path: &str) -> &'static str {
    match (method, path) {
        ("GET", "/") => "/",
        ("GET", "/livez") => "/livez",
        ("GET", "/healthz") => "/healthz",
        ("GET", "/readyz") => "/readyz",
        ("GET", "/packs") => "/packs",
        ("GET", _) if single_path_segment(path, "/artifacts/").is_some() => {
            "/artifacts/{artifact_id}"
        }
        ("GET", "/runs") => "/runs",
        ("GET", _) if latest_artifact_path_parts(path).is_some() => {
            "/runs/{run_id}/artifacts/latest/{artifact_type}"
        }
        ("POST", "/briefs/validate") => "/briefs/validate",
        ("POST", "/briefs/submit") => "/briefs/submit",
        ("GET", _) if single_path_segment(path, "/packs/").is_some() => "/packs/{pack_id}",
        ("GET", _) if single_path_segment(path, "/runs/").is_some() => "/runs/{run_id}",
        ("POST", _) if path.starts_with("/runs/") && path.ends_with("/tasks/next") => {
            "/runs/{run_id}/tasks/next"
        }
        ("POST", _) if path.starts_with("/runs/") && path.ends_with("/worker/once") => {
            "/runs/{run_id}/worker/once"
        }
        ("POST", _) if path.starts_with("/runs/") && path.ends_with("/evaluate-policy") => {
            "/runs/{run_id}/evaluate-policy"
        }
        ("POST", _) if path.starts_with("/runs/") && path.ends_with("/evaluate-quality") => {
            "/runs/{run_id}/evaluate-quality"
        }
        ("POST", _) if path.starts_with("/runs/") && path.ends_with("/export-pr-candidate") => {
            "/runs/{run_id}/export-pr-candidate"
        }
        ("POST", _) if path.starts_with("/runs/") && path.ends_with("/publish-pr-export") => {
            "/runs/{run_id}/publish-pr-export"
        }
        ("POST", _) if path.starts_with("/runs/") && path.ends_with("/draft-pr") => {
            "/runs/{run_id}/draft-pr"
        }
        _ => "unmatched",
    }
}

fn json_response<T: Serialize>(
    status: StatusCode,
    value: &T,
) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response_with_headers(status, value, &[])
}

fn json_response_with_headers<T: Serialize>(
    status: StatusCode,
    value: &T,
    headers: &[Header],
) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec_pretty(value).unwrap_or_else(|error| {
        format!(r#"{{"error":"failed to serialize response: {error}"}}"#).into_bytes()
    });
    let mut response = Response::from_data(body)
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"));

    for response_header in headers.iter().cloned() {
        response = response.with_header(response_header);
    }

    response
}

fn header(name: &str, value: impl AsRef<str>) -> Header {
    Header::from_bytes(name, value.as_ref())
        .unwrap_or_else(|_| panic!("invalid static response header: {name}"))
}

#[derive(Debug, Serialize)]
struct ServiceInfo {
    service: &'static str,
    status: &'static str,
    endpoints: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct LivenessResponse {
    status: &'static str,
    database: &'static str,
    service: &'static str,
}

#[derive(Debug, Serialize)]
struct ReadinessResponse {
    status: &'static str,
    service: &'static str,
    database: &'static str,
    schema: &'static str,
    database_name: Option<String>,
    missing_tables: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize)]
struct RecentRunsResponse {
    count: usize,
    runs: Vec<crate::models::run::RunSummary>,
}

#[derive(Debug)]
struct ListRunsRequest {
    limit: usize,
    filters: RunListFilters,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CreateDraftPrRequest {
    remote_url: Option<String>,
    branch_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ExportPrCandidateRequest {
    branch_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PublishPrExportRequest {
    remote_url: Option<String>,
    push: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        latest_artifact_path_parts, parse_list_runs_request, readiness_payload, route_label,
    };
    use crate::storage::postgres::DatabaseReadiness;
    use anyhow::anyhow;
    use tiny_http::StatusCode;

    #[test]
    fn maps_ready_database_probe_to_ok_response() {
        let (status, payload) = readiness_payload(Ok(DatabaseReadiness {
            database_name: "continuum".to_string(),
            schema_ready: true,
            missing_tables: Vec::new(),
        }));

        assert_eq!(status, StatusCode(200));
        assert_eq!(payload.status, "ok");
        assert_eq!(payload.database, "ready");
        assert_eq!(payload.schema, "ready");
        assert_eq!(payload.database_name.as_deref(), Some("continuum"));
        assert!(payload.error.is_none());
    }

    #[test]
    fn maps_missing_schema_probe_to_service_unavailable() {
        let (status, payload) = readiness_payload(Ok(DatabaseReadiness {
            database_name: "continuum".to_string(),
            schema_ready: false,
            missing_tables: vec!["tasks".to_string()],
        }));

        assert_eq!(status, StatusCode(503));
        assert_eq!(payload.status, "degraded");
        assert_eq!(payload.database, "ready");
        assert_eq!(payload.schema, "missing_tables");
        assert_eq!(payload.missing_tables, vec!["tasks".to_string()]);
    }

    #[test]
    fn maps_database_failure_to_service_unavailable() {
        let (status, payload) = readiness_payload(Err(anyhow!("database offline")));

        assert_eq!(status, StatusCode(503));
        assert_eq!(payload.status, "error");
        assert_eq!(payload.database, "unavailable");
        assert_eq!(payload.schema, "unknown");
        assert!(
            payload
                .error
                .as_deref()
                .is_some_and(|error| error.contains("database offline"))
        );
    }

    #[test]
    fn labels_new_health_routes() {
        assert_eq!(route_label("GET", "/livez"), "/livez");
        assert_eq!(route_label("GET", "/healthz"), "/healthz");
        assert_eq!(route_label("GET", "/readyz"), "/readyz");
    }

    #[test]
    fn labels_latest_artifact_route() {
        assert_eq!(
            route_label(
                "GET",
                "/runs/11111111-1111-1111-1111-111111111111/artifacts/latest/quality_report"
            ),
            "/runs/{run_id}/artifacts/latest/{artifact_type}"
        );
        assert!(latest_artifact_path_parts("/runs/not-a-uuid").is_none());
    }

    #[test]
    fn parses_list_runs_query_filters() {
        let request = parse_list_runs_request(Some("limit=5&status=running&target_pack=cli-tool"))
            .expect("query should parse");

        assert_eq!(request.limit, 5);
        assert_eq!(request.filters.status.as_deref(), Some("executing"));
        assert_eq!(request.filters.target_pack.as_deref(), Some("cli-tool"));
    }
}

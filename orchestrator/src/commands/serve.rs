use anyhow::Context;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tiny_http::{Header, Response, Server, StatusCode};
use uuid::Uuid;

use crate::{
    cli::ServeArgs,
    commands::{
        create_draft_pr, export_pr_candidate, publish_pr_export, run_next_task,
        submit_brief::submit_validated_brief, worker,
    },
    planning::{
        brief_validation::validate_brief_document, pack_catalog::build_pack_catalog,
        packs::PackDefinition, pr_candidate,
    },
    runtime::RuntimeRegistry,
    storage::postgres::PostgresRunStore,
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
        let path = request.url().split('?').next().unwrap_or("/");
        let method = request.method().as_str().to_string();

        let response = match (method.as_str(), path) {
            ("GET", "/") => json_response(
                StatusCode(200),
                &ServiceInfo {
                    service: "catalyst-continuum-orchestrator",
                    status: "ok",
                    endpoints: vec![
                        "/",
                        "/healthz",
                        "/packs",
                        "/packs/{pack_id}",
                        "/runs",
                        "/runs/{run_id}",
                        "POST /runs/{run_id}/tasks/next",
                        "POST /runs/{run_id}/worker/once",
                        "POST /runs/{run_id}/export-pr-candidate",
                        "POST /runs/{run_id}/publish-pr-export",
                        "POST /runs/{run_id}/draft-pr",
                        "POST /briefs/validate",
                        "POST /briefs/submit",
                    ],
                },
            ),
            ("GET", "/healthz") => json_response(
                StatusCode(200),
                &HealthResponse {
                    status: "ok",
                    database: "ready",
                },
            ),
            ("GET", "/packs") => match build_pack_catalog() {
                Ok(catalog) => json_response(StatusCode(200), &catalog),
                Err(error) => json_response(
                    StatusCode(500),
                    &ErrorResponse {
                        error: format!("failed to build pack catalog: {error}"),
                    },
                ),
            },
            ("GET", "/runs") => match store.list_runs(DEFAULT_RUN_LIST_LIMIT) {
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
            },
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

        if let Err(error) = request.respond(response) {
            tracing::warn!(%error, "failed to send HTTP response");
        }
    }

    Ok(())
}

fn parse_run_id(value: &str) -> anyhow::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| anyhow::anyhow!("invalid run id `{value}`: {error}"))
}

fn parse_run_action_path(path: &str, suffix: &str) -> anyhow::Result<Uuid> {
    let run_id = path
        .strip_prefix("/runs/")
        .and_then(|value| value.strip_suffix(suffix))
        .ok_or_else(|| anyhow::anyhow!("invalid run action path: {path}"))?;

    parse_run_id(run_id)
}

fn single_path_segment<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)
        .filter(|value| !value.is_empty() && !value.contains('/'))
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
struct HealthResponse {
    status: &'static str,
    database: &'static str,
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

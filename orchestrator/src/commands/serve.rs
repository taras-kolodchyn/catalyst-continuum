use anyhow::Context;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::HashMap, time::Instant};
use tiny_http::{Header, Request, Response, Server, StatusCode};
use uuid::Uuid;

use crate::{
    cli::ServeArgs,
    commands::{
        create_draft_pr, describe_ai_gateway_status, describe_artifact,
        describe_github_default_branch_state, describe_github_webhook_action_report,
        describe_github_webhook_receipt, describe_latest_artifact,
        describe_repository_signal_payload, evaluate_run_policy, evaluate_run_quality,
        export_pr_candidate, publish_pr_export, run_next_github_webhook_action, run_next_task,
        submit_brief::submit_validated_brief, worker,
    },
    config::{InstanceConfigReport, load_github_app_webhook_secret},
    coordination,
    github_webhook_routing::evaluate_github_webhook_route,
    github_webhooks::{GitHubWebhookErrorKind, GitHubWebhookHeaders, ingest_github_webhook},
    models::repository_signal::{RepositorySignalListFilters, RepositorySignalSummary},
    models::webhook::{
        GitHubWebhookActionRequestDraft, GitHubWebhookActionRequestListFilters,
        GitHubWebhookActionRequestSummary, GitHubWebhookDeliveryDraft,
        GitHubWebhookDeliverySummary, GitHubWebhookListFilters,
    },
    operator_ui,
    planning::{
        brief_validation::validate_brief_document_with_external_mcp_servers,
        pack_catalog::build_pack_catalog, packs::PackDefinition, pr_candidate,
    },
    runtime::RuntimeRegistry,
    storage::postgres::{DatabaseReadiness, PostgresRunStore, RunEventListFilters, RunListFilters},
    telemetry,
};

const DEFAULT_RUN_LIST_LIMIT: usize = 20;
const DEFAULT_RUN_EVENT_LIST_LIMIT: usize = 20;
const DEFAULT_WEBHOOK_LIST_LIMIT: usize = 20;
const DEFAULT_WEBHOOK_ACTION_REQUEST_LIST_LIMIT: usize = 20;
const DEFAULT_REPOSITORY_SIGNAL_LIST_LIMIT: usize = 20;

pub fn execute(args: ServeArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let instance_config = InstanceConfigReport::load(
        args.runtime_providers_file.as_deref(),
        args.mcp_servers_file.as_deref(),
        args.ai_gateway_file.as_deref(),
    )?;
    let runtime_registry =
        RuntimeRegistry::from_runtime_providers_config(&instance_config.runtime_providers);
    let github_webhook_secret = load_github_app_webhook_secret();

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
                        "/ui",
                        "/livez",
                        "/healthz",
                        "/readyz",
                        "/config",
                        "/ai-gateway/status",
                        "/github/webhooks",
                        "/github/webhooks/{delivery_id}",
                        "/github/webhooks/{delivery_id}/receipt",
                        "/github/webhook-actions",
                        "/github/webhook-actions/{request_id}",
                        "/github/webhook-actions/{request_id}/report",
                        "/github/repositories/{owner}/{repo}/default-branch-state",
                        "/repository-signals",
                        "/repository-signals/{signal_id}",
                        "/repository-signals/{signal_id}/payload",
                        "POST /github/webhooks",
                        "POST /github/webhook-actions/next",
                        "/packs",
                        "/packs/{pack_id}",
                        "/artifacts/{artifact_id}",
                        "/runs",
                        "/runs/{run_id}",
                        "/runs/{run_id}/events",
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
            ("GET", _) if operator_ui::route_label(path).is_some() => operator_ui::response(path)
                .expect("operator UI route guard should resolve a static response"),
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
            ("GET", "/config") => json_response(StatusCode(200), &instance_config),
            ("GET", "/ai-gateway/status") => {
                let api_key = describe_ai_gateway_status::gateway_api_key_from_env();
                let probe_base_url = describe_ai_gateway_status::gateway_probe_base_url_from_env();
                let report = describe_ai_gateway_status::describe_ai_gateway_status(
                    &instance_config.ai_gateway,
                    2_000,
                    api_key.as_deref(),
                    probe_base_url.as_deref(),
                );
                json_response(
                    if report.ready {
                        StatusCode(200)
                    } else {
                        StatusCode(503)
                    },
                    &report,
                )
            }
            ("GET", "/github/webhooks") => match parse_list_github_webhooks_request(query) {
                Ok(list_request) => match store
                    .list_github_webhook_deliveries(list_request.limit, &list_request.filters)
                {
                    Ok(deliveries) => json_response(
                        StatusCode(200),
                        &RecentGithubWebhookDeliveriesResponse {
                            count: deliveries.len(),
                            deliveries,
                        },
                    ),
                    Err(error) => json_response(
                        StatusCode(500),
                        &ErrorResponse {
                            error: format!("failed to list github webhook deliveries: {error}"),
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
            ("GET", "/github/webhook-actions") => {
                match parse_list_github_webhook_action_requests_request(query) {
                    Ok(list_request) => match store.list_github_webhook_action_requests(
                        list_request.limit,
                        &list_request.filters,
                    ) {
                        Ok(requests) => json_response(
                            StatusCode(200),
                            &RecentGithubWebhookActionRequestsResponse {
                                count: requests.len(),
                                requests,
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!(
                                    "failed to list github webhook action requests: {error}"
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
            ("GET", "/repository-signals") => match parse_list_repository_signals_request(query) {
                Ok(list_request) => {
                    match store.list_repository_signals(list_request.limit, &list_request.filters) {
                        Ok(signals) => json_response(
                            StatusCode(200),
                            &RecentRepositorySignalsResponse {
                                count: signals.len(),
                                signals,
                            },
                        ),
                        Err(error) => json_response(
                            StatusCode(500),
                            &ErrorResponse {
                                error: format!("failed to list repository signals: {error}"),
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
            ("POST", "/github/webhook-actions/next") => {
                match read_optional_json_body::<RunNextGithubWebhookActionRequest>(&mut request) {
                    Ok(run_request) => {
                        match run_next_github_webhook_action::execute_next_github_webhook_action(
                            &mut store,
                            &args.artifact_root,
                            run_request.action.as_deref(),
                        ) {
                            Ok(outcome) => {
                                let status = if matches!(
                                    outcome,
                                    run_next_github_webhook_action::NextGitHubWebhookActionExecution::Idle(_)
                                ) {
                                    StatusCode(200)
                                } else {
                                    StatusCode(202)
                                };
                                json_response(status, &outcome)
                            }
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to execute github webhook action request: {error}"
                                    ),
                                },
                            ),
                        }
                    }
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: format!(
                                "failed to parse github webhook action request body: {error}"
                            ),
                        },
                    ),
                }
            }
            ("POST", "/github/webhooks") => {
                let webhook_started_at = Instant::now();
                let event = header_value(request.headers(), "X-GitHub-Event")
                    .unwrap_or("unknown")
                    .to_string();
                let response = match parse_github_webhook_headers(request.headers()) {
                    Ok(headers) => match read_request_body_bytes(&mut request) {
                        Ok(body) => match ingest_github_webhook(
                            &headers,
                            &body,
                            &args.artifact_root,
                            github_webhook_secret.as_deref(),
                        ) {
                            Ok(summary) => {
                                let routing = evaluate_github_webhook_route(
                                    &summary,
                                    &instance_config.github_app,
                                );
                                match GitHubWebhookDeliveryDraft::from_receipt_summary(
                                    &summary, &routing,
                                ) {
                                    Ok(delivery_draft) => {
                                        match store.upsert_github_webhook_delivery(&delivery_draft)
                                        {
                                            Ok(delivery) => {
                                                if let Some(action_request_draft) =
                                                    GitHubWebhookActionRequestDraft::from_delivery_summary(
                                                        &delivery,
                                                    )
                                                {
                                                    match store
                                                        .upsert_github_webhook_action_request(
                                                            &action_request_draft,
                                                        )
                                                    {
                                                        Ok(_) => github_webhook_response(&delivery),
                                                        Err(error) => json_response(
                                                            StatusCode(500),
                                                            &ErrorResponse {
                                                                error: format!(
                                                                    "failed to persist github webhook action request: {error}"
                                                                ),
                                                            },
                                                        ),
                                                    }
                                                } else {
                                                    github_webhook_response(&delivery)
                                                }
                                            }
                                            Err(error) => json_response(
                                                StatusCode(500),
                                                &ErrorResponse {
                                                    error: format!(
                                                        "failed to persist github webhook delivery: {error}"
                                                    ),
                                                },
                                            ),
                                        }
                                    }
                                    Err(error) => json_response(
                                        StatusCode(500),
                                        &ErrorResponse {
                                            error: format!(
                                                "failed to build github webhook delivery record: {error}"
                                            ),
                                        },
                                    ),
                                }
                            }
                            Err(error) => json_response(
                                webhook_error_status(error.kind()),
                                &ErrorResponse {
                                    error: error.message().to_string(),
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
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                };
                let outcome = if response.status_code().0 < 400 {
                    "accepted"
                } else if response.status_code().0 == 401 {
                    "rejected"
                } else if response.status_code().0 == 503 {
                    "unavailable"
                } else {
                    "invalid"
                };
                telemetry::record_webhook_delivery(
                    "github",
                    event.as_str(),
                    outcome,
                    webhook_started_at.elapsed(),
                );
                response
            }
            ("GET", _) if github_webhook_receipt_path_delivery_id(path).is_some() => {
                let delivery_id = github_webhook_receipt_path_delivery_id(path)
                    .expect("github webhook receipt path guard should provide delivery id");
                match store.fetch_github_webhook_delivery(delivery_id) {
                    Ok(Some(delivery)) => {
                        match describe_github_webhook_receipt::describe_github_webhook_receipt(
                            &delivery,
                        ) {
                            Ok(receipt) => json_response(StatusCode(200), &receipt),
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to fetch github webhook receipt for {delivery_id}: {error}"
                                    ),
                                },
                            ),
                        }
                    }
                    Ok(None) => json_response(
                        StatusCode(404),
                        &ErrorResponse {
                            error: format!("github webhook delivery not found: {delivery_id}"),
                        },
                    ),
                    Err(error) => json_response(
                        StatusCode(500),
                        &ErrorResponse {
                            error: format!(
                                "failed to fetch github webhook delivery {delivery_id}: {error}"
                            ),
                        },
                    ),
                }
            }
            ("GET", _) if single_path_segment(path, "/github/webhooks/").is_some() => {
                let delivery_id = single_path_segment(path, "/github/webhooks/")
                    .expect("github webhook path guard should provide a single path segment");
                match store.fetch_github_webhook_delivery(delivery_id) {
                    Ok(Some(delivery)) => json_response(StatusCode(200), &delivery),
                    Ok(None) => json_response(
                        StatusCode(404),
                        &ErrorResponse {
                            error: format!("github webhook delivery not found: {delivery_id}"),
                        },
                    ),
                    Err(error) => json_response(
                        StatusCode(500),
                        &ErrorResponse {
                            error: format!(
                                "failed to fetch github webhook delivery {delivery_id}: {error}"
                            ),
                        },
                    ),
                }
            }
            ("GET", _) if single_path_segment(path, "/github/webhook-actions/").is_some() => {
                let request_id = single_path_segment(path, "/github/webhook-actions/").expect(
                    "github webhook action path guard should provide a single path segment",
                );
                match store.fetch_github_webhook_action_request(request_id) {
                    Ok(Some(request)) => json_response(StatusCode(200), &request),
                    Ok(None) => json_response(
                        StatusCode(404),
                        &ErrorResponse {
                            error: format!("github webhook action request not found: {request_id}"),
                        },
                    ),
                    Err(error) => json_response(
                        StatusCode(500),
                        &ErrorResponse {
                            error: format!(
                                "failed to fetch github webhook action request {request_id}: {error}"
                            ),
                        },
                    ),
                }
            }
            ("GET", _) if github_webhook_action_report_path_request_id(path).is_some() => {
                let request_id = github_webhook_action_report_path_request_id(path)
                    .expect("github webhook action report path guard should provide request id");
                match store.fetch_github_webhook_action_request(request_id) {
                    Ok(Some(request)) => {
                        match describe_github_webhook_action_report::describe_github_webhook_action_report(&request) {
                            Ok(report) => json_response(StatusCode(200), &report),
                            Err(error) => {
                                let status = if request.report_path.is_none() {
                                    StatusCode(409)
                                } else {
                                    StatusCode(500)
                                };
                                json_response(
                                    status,
                                    &ErrorResponse {
                                        error: format!(
                                            "failed to fetch github webhook action report for {request_id}: {error}"
                                        ),
                                    },
                                )
                            }
                        }
                    }
                    Ok(None) => json_response(
                        StatusCode(404),
                        &ErrorResponse {
                            error: format!("github webhook action request not found: {request_id}"),
                        },
                    ),
                    Err(error) => json_response(
                        StatusCode(500),
                        &ErrorResponse {
                            error: format!(
                                "failed to fetch github webhook action request {request_id}: {error}"
                            ),
                        },
                    ),
                }
            }
            ("GET", _) if github_default_branch_state_path_parts(path).is_some() => {
                let (owner, repo) = github_default_branch_state_path_parts(path)
                    .expect("github default-branch state path guard should provide owner and repo");
                let repository_full_name = format!("{owner}/{repo}");
                match describe_github_default_branch_state::describe_github_default_branch_state(
                    &args.artifact_root,
                    "github",
                    &repository_full_name,
                ) {
                    Ok(state) => json_response(StatusCode(200), &state),
                    Err(error) => {
                        let status = if error
                            .to_string()
                            .contains("github default-branch state not found")
                        {
                            StatusCode(404)
                        } else {
                            StatusCode(500)
                        };
                        json_response(
                            status,
                            &ErrorResponse {
                                error: format!(
                                    "failed to fetch github default-branch state for {repository_full_name}: {error}"
                                ),
                            },
                        )
                    }
                }
            }
            ("GET", _) if repository_signal_payload_path_signal_id(path).is_some() => {
                let signal_id = repository_signal_payload_path_signal_id(path).expect(
                    "repository signal payload path guard should provide a single path segment",
                );
                match store.fetch_repository_signal(signal_id) {
                    Ok(Some(signal)) => {
                        match describe_repository_signal_payload::describe_repository_signal_payload(
                            &signal,
                        ) {
                            Ok(payload) => json_response(StatusCode(200), &payload),
                            Err(error) => json_response(
                                StatusCode(500),
                                &ErrorResponse {
                                    error: format!(
                                        "failed to fetch repository signal payload for {signal_id}: {error}"
                                    ),
                                },
                            ),
                        }
                    }
                    Ok(None) => json_response(
                        StatusCode(404),
                        &ErrorResponse {
                            error: format!("repository signal not found: {signal_id}"),
                        },
                    ),
                    Err(error) => json_response(
                        StatusCode(500),
                        &ErrorResponse {
                            error: format!(
                                "failed to fetch repository signal {signal_id}: {error}"
                            ),
                        },
                    ),
                }
            }
            ("GET", _) if single_path_segment(path, "/repository-signals/").is_some() => {
                let signal_id = single_path_segment(path, "/repository-signals/")
                    .expect("repository signal path guard should provide a single path segment");
                match store.fetch_repository_signal(signal_id) {
                    Ok(Some(signal)) => json_response(StatusCode(200), &signal),
                    Ok(None) => json_response(
                        StatusCode(404),
                        &ErrorResponse {
                            error: format!("repository signal not found: {signal_id}"),
                        },
                    ),
                    Err(error) => json_response(
                        StatusCode(500),
                        &ErrorResponse {
                            error: format!(
                                "failed to fetch repository signal {signal_id}: {error}"
                            ),
                        },
                    ),
                }
            }
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
            ("GET", _) if run_events_path_parts(path).is_some() => {
                match parse_run_events_path(path) {
                    Ok(run_id) => match parse_list_run_events_request(query) {
                        Ok(list_request) => match store.fetch_run_summary(run_id) {
                            Ok(Some(_)) => match store.list_run_events(
                                run_id,
                                list_request.limit,
                                &list_request.filters,
                            ) {
                                Ok(events) => json_response(
                                    StatusCode(200),
                                    &RecentRunEventsResponse {
                                        count: events.len(),
                                        events,
                                    },
                                ),
                                Err(error) => json_response(
                                    StatusCode(500),
                                    &ErrorResponse {
                                        error: format!(
                                            "failed to list events for run {run_id}: {error}"
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
                    },
                    Err(error) => json_response(
                        StatusCode(400),
                        &ErrorResponse {
                            error: error.to_string(),
                        },
                    ),
                }
            }
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
                Ok(body) => match validate_brief_document_with_external_mcp_servers(
                    &body,
                    "http:POST /briefs/validate",
                    &instance_config.external_mcp_servers,
                ) {
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
                Ok(body) => match validate_brief_document_with_external_mcp_servers(
                    &body,
                    "http:POST /briefs/submit",
                    &instance_config.external_mcp_servers,
                ) {
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
                                            promotion_error_status(&error),
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
                                        promotion_error_status(&error),
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
                                                promotion_error_status(&error),
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

fn promotion_error_status(error: &anyhow::Error) -> StatusCode {
    if coordination::is_promotion_lock_conflict(error) {
        return StatusCode(409);
    }

    StatusCode(500)
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

fn run_events_path_parts(path: &str) -> Option<&str> {
    let run_id = path.strip_prefix("/runs/")?.strip_suffix("/events")?;
    if run_id.is_empty() || run_id.contains('/') {
        return None;
    }

    Some(run_id)
}

fn parse_run_events_path(path: &str) -> anyhow::Result<Uuid> {
    let run_id = run_events_path_parts(path)
        .ok_or_else(|| anyhow::anyhow!("invalid run events path: {path}"))?;

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

fn github_webhook_action_report_path_request_id(path: &str) -> Option<&str> {
    let request_id = path
        .strip_prefix("/github/webhook-actions/")?
        .strip_suffix("/report")?;
    if request_id.is_empty() || request_id.contains('/') {
        return None;
    }

    Some(request_id)
}

fn github_webhook_receipt_path_delivery_id(path: &str) -> Option<&str> {
    let delivery_id = path
        .strip_prefix("/github/webhooks/")?
        .strip_suffix("/receipt")?;
    if delivery_id.is_empty() || delivery_id.contains('/') {
        return None;
    }

    Some(delivery_id)
}

fn github_default_branch_state_path_parts(path: &str) -> Option<(&str, &str)> {
    let remainder = path
        .strip_prefix("/github/repositories/")?
        .strip_suffix("/default-branch-state")?;
    let mut parts = remainder.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }

    Some((owner, repo))
}

fn repository_signal_payload_path_signal_id(path: &str) -> Option<&str> {
    let signal_id = path
        .strip_prefix("/repository-signals/")?
        .strip_suffix("/payload")?;
    if signal_id.is_empty() || signal_id.contains('/') {
        return None;
    }

    Some(signal_id)
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

fn read_request_body_bytes(request: &mut Request) -> std::io::Result<Vec<u8>> {
    let mut body = Vec::new();
    request.as_reader().read_to_end(&mut body)?;
    Ok(body)
}

fn read_request_body(request: &mut Request) -> anyhow::Result<String> {
    let body = read_request_body_bytes(request).context("failed to read request body")?;
    String::from_utf8(body).context("request body is not valid UTF-8")
}

fn parse_github_webhook_headers(headers: &[Header]) -> anyhow::Result<GitHubWebhookHeaders> {
    Ok(GitHubWebhookHeaders {
        event: required_header_value(headers, "X-GitHub-Event")?.to_string(),
        delivery_id: required_header_value(headers, "X-GitHub-Delivery")?.to_string(),
        signature_sha256: required_header_value(headers, "X-Hub-Signature-256")?.to_string(),
    })
}

fn required_header_value<'a>(headers: &'a [Header], name: &'static str) -> anyhow::Result<&'a str> {
    header_value(headers, name)
        .ok_or_else(|| anyhow::anyhow!("missing required HTTP header: {name}"))
}

fn header_value<'a>(headers: &'a [Header], name: &'static str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
}

fn github_webhook_response(
    summary: &GitHubWebhookDeliverySummary,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let status = if summary.outcome == "ping" {
        StatusCode(200)
    } else {
        StatusCode(202)
    };

    json_response_with_headers(
        status,
        summary,
        &[header(
            "Location",
            format!("/github/webhooks/{}", summary.delivery_id),
        )],
    )
}

fn webhook_error_status(kind: GitHubWebhookErrorKind) -> StatusCode {
    match kind {
        GitHubWebhookErrorKind::BadRequest => StatusCode(400),
        GitHubWebhookErrorKind::Unauthorized => StatusCode(401),
        GitHubWebhookErrorKind::ServiceUnavailable => StatusCode(503),
        GitHubWebhookErrorKind::Internal => StatusCode(500),
    }
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

fn parse_list_run_events_request(query: Option<&str>) -> anyhow::Result<ListRunEventsRequest> {
    let query_pairs = parse_query_pairs(query);
    let limit = match query_pairs.get("limit") {
        Some(value) if !value.trim().is_empty() => {
            value.trim().parse::<usize>().map_err(|error| {
                anyhow::anyhow!("invalid run_events.limit `{}`: {error}", value.trim())
            })?
        }
        _ => DEFAULT_RUN_EVENT_LIST_LIMIT,
    };
    let task_id = match query_pairs.get("task_id") {
        Some(value) if !value.trim().is_empty() => {
            Some(Uuid::parse_str(value.trim()).map_err(|error| {
                anyhow::anyhow!("invalid run_events.task_id `{}`: {error}", value.trim())
            })?)
        }
        _ => None,
    };
    let filters = RunEventListFilters::from_inputs(
        query_pairs.get("event_type").map(String::as_str),
        task_id,
    );

    Ok(ListRunEventsRequest { limit, filters })
}

fn parse_list_github_webhooks_request(
    query: Option<&str>,
) -> anyhow::Result<ListGithubWebhooksRequest> {
    let query_pairs = parse_query_pairs(query);
    let limit = match query_pairs.get("limit") {
        Some(value) if !value.trim().is_empty() => {
            value.trim().parse::<usize>().map_err(|error| {
                anyhow::anyhow!("invalid github_webhooks.limit `{}`: {error}", value.trim())
            })?
        }
        _ => DEFAULT_WEBHOOK_LIST_LIMIT,
    };
    let filters =
        GitHubWebhookListFilters::from_inputs(query_pairs.get("event").map(String::as_str));

    Ok(ListGithubWebhooksRequest { limit, filters })
}

fn parse_list_github_webhook_action_requests_request(
    query: Option<&str>,
) -> anyhow::Result<ListGithubWebhookActionRequestsRequest> {
    let query_pairs = parse_query_pairs(query);
    let limit = match query_pairs.get("limit") {
        Some(value) if !value.trim().is_empty() => {
            value.trim().parse::<usize>().map_err(|error| {
                anyhow::anyhow!(
                    "invalid github_webhook_actions.limit `{}`: {error}",
                    value.trim()
                )
            })?
        }
        _ => DEFAULT_WEBHOOK_ACTION_REQUEST_LIST_LIMIT,
    };
    let filters = GitHubWebhookActionRequestListFilters::from_inputs(
        query_pairs.get("status").map(String::as_str),
        query_pairs.get("action").map(String::as_str),
    );

    Ok(ListGithubWebhookActionRequestsRequest { limit, filters })
}

fn parse_list_repository_signals_request(
    query: Option<&str>,
) -> anyhow::Result<ListRepositorySignalsRequest> {
    let query_pairs = parse_query_pairs(query);
    let limit = match query_pairs.get("limit") {
        Some(value) if !value.trim().is_empty() => {
            value.trim().parse::<usize>().map_err(|error| {
                anyhow::anyhow!(
                    "invalid repository_signals.limit `{}`: {error}",
                    value.trim()
                )
            })?
        }
        _ => DEFAULT_REPOSITORY_SIGNAL_LIST_LIMIT,
    };
    let filters = RepositorySignalListFilters::from_inputs(
        query_pairs.get("status").map(String::as_str),
        query_pairs.get("signal_kind").map(String::as_str),
        query_pairs.get("repository_full_name").map(String::as_str),
    );

    Ok(ListRepositorySignalsRequest { limit, filters })
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
    if method == "GET"
        && let Some(label) = operator_ui::route_label(path)
    {
        return label;
    }

    match (method, path) {
        ("GET", "/") => "/",
        ("GET", "/livez") => "/livez",
        ("GET", "/healthz") => "/healthz",
        ("GET", "/readyz") => "/readyz",
        ("GET", "/config") => "/config",
        ("GET", "/ai-gateway/status") => "/ai-gateway/status",
        ("GET", "/github/webhooks") => "/github/webhooks",
        ("GET", "/github/webhook-actions") => "/github/webhook-actions",
        ("GET", _) if github_webhook_receipt_path_delivery_id(path).is_some() => {
            "/github/webhooks/{delivery_id}/receipt"
        }
        ("GET", _) if github_webhook_action_report_path_request_id(path).is_some() => {
            "/github/webhook-actions/{request_id}/report"
        }
        ("GET", _) if github_default_branch_state_path_parts(path).is_some() => {
            "/github/repositories/{owner}/{repo}/default-branch-state"
        }
        ("GET", "/repository-signals") => "/repository-signals",
        ("GET", _) if repository_signal_payload_path_signal_id(path).is_some() => {
            "/repository-signals/{signal_id}/payload"
        }
        ("POST", "/github/webhooks") => "/github/webhooks",
        ("POST", "/github/webhook-actions/next") => "/github/webhook-actions/next",
        ("GET", _) if single_path_segment(path, "/github/webhooks/").is_some() => {
            "/github/webhooks/{delivery_id}"
        }
        ("GET", _) if single_path_segment(path, "/github/webhook-actions/").is_some() => {
            "/github/webhook-actions/{request_id}"
        }
        ("GET", _) if single_path_segment(path, "/repository-signals/").is_some() => {
            "/repository-signals/{signal_id}"
        }
        ("GET", "/packs") => "/packs",
        ("GET", _) if single_path_segment(path, "/artifacts/").is_some() => {
            "/artifacts/{artifact_id}"
        }
        ("GET", "/runs") => "/runs",
        ("GET", _) if run_events_path_parts(path).is_some() => "/runs/{run_id}/events",
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

#[derive(Debug, Serialize)]
struct RecentRunEventsResponse {
    count: usize,
    events: Vec<crate::models::run_event::RunEventSummary>,
}

#[derive(Debug)]
struct ListGithubWebhooksRequest {
    limit: usize,
    filters: GitHubWebhookListFilters,
}

#[derive(Debug)]
struct ListGithubWebhookActionRequestsRequest {
    limit: usize,
    filters: GitHubWebhookActionRequestListFilters,
}

#[derive(Debug)]
struct ListRepositorySignalsRequest {
    limit: usize,
    filters: RepositorySignalListFilters,
}

#[derive(Debug, Serialize)]
struct RecentGithubWebhookDeliveriesResponse {
    count: usize,
    deliveries: Vec<GitHubWebhookDeliverySummary>,
}

#[derive(Debug, Serialize)]
struct RecentGithubWebhookActionRequestsResponse {
    count: usize,
    requests: Vec<GitHubWebhookActionRequestSummary>,
}

#[derive(Debug, Serialize)]
struct RecentRepositorySignalsResponse {
    count: usize,
    signals: Vec<RepositorySignalSummary>,
}

#[derive(Debug)]
struct ListRunsRequest {
    limit: usize,
    filters: RunListFilters,
}

#[derive(Debug)]
struct ListRunEventsRequest {
    limit: usize,
    filters: RunEventListFilters,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RunNextGithubWebhookActionRequest {
    action: Option<String>,
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
        github_default_branch_state_path_parts, github_webhook_action_report_path_request_id,
        github_webhook_receipt_path_delivery_id, latest_artifact_path_parts,
        parse_list_github_webhook_action_requests_request, parse_list_github_webhooks_request,
        parse_list_repository_signals_request, parse_list_run_events_request,
        parse_list_runs_request, readiness_payload, repository_signal_payload_path_signal_id,
        route_label,
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
        assert_eq!(route_label("GET", "/ui"), "/ui");
        assert_eq!(route_label("GET", "/ui/app.js"), "/ui/app.js");
        assert_eq!(route_label("GET", "/ui/styles.css"), "/ui/styles.css");
        assert_eq!(route_label("GET", "/livez"), "/livez");
        assert_eq!(route_label("GET", "/healthz"), "/healthz");
        assert_eq!(route_label("GET", "/readyz"), "/readyz");
        assert_eq!(route_label("GET", "/config"), "/config");
        assert_eq!(
            route_label("GET", "/ai-gateway/status"),
            "/ai-gateway/status"
        );
        assert_eq!(route_label("GET", "/github/webhooks"), "/github/webhooks");
        assert_eq!(
            route_label("GET", "/github/webhook-actions"),
            "/github/webhook-actions"
        );
        assert_eq!(
            route_label("GET", "/repository-signals"),
            "/repository-signals"
        );
        assert_eq!(route_label("POST", "/github/webhooks"), "/github/webhooks");
        assert_eq!(
            route_label("POST", "/github/webhook-actions/next"),
            "/github/webhook-actions/next"
        );
        assert_eq!(
            route_label("GET", "/github/webhooks/delivery-1"),
            "/github/webhooks/{delivery_id}"
        );
        assert_eq!(
            route_label("GET", "/github/webhooks/delivery-1/receipt"),
            "/github/webhooks/{delivery_id}/receipt"
        );
        assert_eq!(
            route_label("GET", "/github/webhook-actions/request-1"),
            "/github/webhook-actions/{request_id}"
        );
        assert_eq!(
            route_label("GET", "/github/webhook-actions/request-1/report"),
            "/github/webhook-actions/{request_id}/report"
        );
        assert_eq!(
            route_label(
                "GET",
                "/github/repositories/smartit/catalyst-continuum/default-branch-state"
            ),
            "/github/repositories/{owner}/{repo}/default-branch-state"
        );
        assert_eq!(
            route_label("GET", "/repository-signals/signal-1"),
            "/repository-signals/{signal_id}"
        );
        assert_eq!(
            route_label("GET", "/repository-signals/signal-1/payload"),
            "/repository-signals/{signal_id}/payload"
        );
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
    fn parses_github_auxiliary_paths() {
        assert_eq!(
            github_webhook_action_report_path_request_id(
                "/github/webhook-actions/request-1/report"
            ),
            Some("request-1")
        );
        assert_eq!(
            github_webhook_receipt_path_delivery_id("/github/webhooks/delivery-1/receipt"),
            Some("delivery-1")
        );
        assert_eq!(
            github_default_branch_state_path_parts(
                "/github/repositories/smartit/catalyst-continuum/default-branch-state"
            ),
            Some(("smartit", "catalyst-continuum"))
        );
        assert_eq!(
            repository_signal_payload_path_signal_id("/repository-signals/signal-1/payload"),
            Some("signal-1")
        );
        assert!(
            github_default_branch_state_path_parts("/github/repositories/only-owner").is_none()
        );
        assert!(github_webhook_receipt_path_delivery_id("/github/webhooks/delivery-1").is_none());
        assert!(repository_signal_payload_path_signal_id("/repository-signals/signal-1").is_none());
    }

    #[test]
    fn labels_run_events_route() {
        assert_eq!(
            route_label("GET", "/runs/11111111-1111-1111-1111-111111111111/events"),
            "/runs/{run_id}/events"
        );
    }

    #[test]
    fn parses_list_runs_query_filters() {
        let request = parse_list_runs_request(Some("limit=5&status=running&target_pack=cli-tool"))
            .expect("query should parse");

        assert_eq!(request.limit, 5);
        assert_eq!(request.filters.status.as_deref(), Some("executing"));
        assert_eq!(request.filters.target_pack.as_deref(), Some("cli-tool"));
    }

    #[test]
    fn parses_list_run_events_query_filters() {
        let request = parse_list_run_events_request(Some(
            "limit=5&event_type=task_succeeded&task_id=11111111-1111-1111-1111-111111111111",
        ))
        .expect("query should parse");

        assert_eq!(request.limit, 5);
        assert_eq!(
            request.filters.event_type.as_deref(),
            Some("task_succeeded")
        );
        assert_eq!(
            request.filters.task_id,
            Some(
                uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("valid uuid")
            )
        );
    }

    #[test]
    fn parses_list_github_webhooks_query_filters() {
        let request = parse_list_github_webhooks_request(Some("limit=5&event=ping"))
            .expect("query should parse");

        assert_eq!(request.limit, 5);
        assert_eq!(request.filters.event.as_deref(), Some("ping"));
    }

    #[test]
    fn parses_list_github_webhook_action_requests_query_filters() {
        let request = parse_list_github_webhook_action_requests_request(Some(
            "limit=5&status=pending&action=sync_default_branch",
        ))
        .expect("query should parse");

        assert_eq!(request.limit, 5);
        assert_eq!(request.filters.status.as_deref(), Some("pending"));
        assert_eq!(
            request.filters.action.as_deref(),
            Some("sync_default_branch")
        );
    }

    #[test]
    fn parses_list_repository_signals_query_filters() {
        let request = parse_list_repository_signals_request(Some(
            "limit=5&status=pending&signal_kind=default_branch_updated&repository_full_name=smartit/catalyst-continuum",
        ))
        .expect("query should parse");

        assert_eq!(request.limit, 5);
        assert_eq!(request.filters.status.as_deref(), Some("pending"));
        assert_eq!(
            request.filters.signal_kind.as_deref(),
            Some("default_branch_updated")
        );
        assert_eq!(
            request.filters.repository_full_name.as_deref(),
            Some("smartit/catalyst-continuum")
        );
    }
}

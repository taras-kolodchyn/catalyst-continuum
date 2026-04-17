use serde::Serialize;
use tiny_http::{Header, Response, Server, StatusCode};

use crate::{
    cli::ServeArgs,
    planning::{
        brief_validation::validate_brief_document, pack_catalog::build_pack_catalog,
        packs::PackDefinition,
    },
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: ServeArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

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
                        "POST /briefs/validate",
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
            ("GET", _) if path.starts_with("/packs/") => {
                let pack_id = path.trim_start_matches("/packs/");
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

fn read_request_body(request: &mut tiny_http::Request) -> std::io::Result<String> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    Ok(body)
}

fn json_response<T: Serialize>(
    status: StatusCode,
    value: &T,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec_pretty(value).unwrap_or_else(|error| {
        format!(r#"{{"error":"failed to serialize response: {error}"}}"#).into_bytes()
    });
    let header = Header::from_bytes("Content-Type", "application/json; charset=utf-8")
        .expect("static content-type header should be valid");

    Response::from_data(body)
        .with_status_code(status)
        .with_header(header)
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

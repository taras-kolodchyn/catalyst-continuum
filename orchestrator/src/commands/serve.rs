use serde::Serialize;
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::{
    cli::ServeArgs,
    planning::{pack_catalog::build_pack_catalog, packs::PackDefinition},
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

    for request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("/");

        let response = match (request.method(), path) {
            (&Method::Get, "/") => json_response(
                StatusCode(200),
                &ServiceInfo {
                    service: "catalyst-continuum-orchestrator",
                    status: "ok",
                    endpoints: vec!["/", "/healthz", "/packs", "/packs/{pack_id}"],
                },
            ),
            (&Method::Get, "/healthz") => json_response(
                StatusCode(200),
                &HealthResponse {
                    status: "ok",
                    database: "ready",
                },
            ),
            (&Method::Get, "/packs") => match build_pack_catalog() {
                Ok(catalog) => json_response(StatusCode(200), &catalog),
                Err(error) => json_response(
                    StatusCode(500),
                    &ErrorResponse {
                        error: format!("failed to build pack catalog: {error}"),
                    },
                ),
            },
            (&Method::Get, _) if path.starts_with("/packs/") => {
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
            _ => json_response(
                StatusCode(404),
                &ErrorResponse {
                    error: format!("route not found: {} {}", request.method().as_str(), path),
                },
            ),
        };

        if let Err(error) = request.respond(response) {
            tracing::warn!(%error, "failed to send HTTP response");
        }
    }

    Ok(())
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

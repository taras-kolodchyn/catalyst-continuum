use std::{io::Write, net::TcpListener};

use anyhow::Context;

use crate::cli::ServeArgs;

pub fn execute(args: ServeArgs) -> anyhow::Result<()> {
    let listener = TcpListener::bind(&args.bind_addr)
        .with_context(|| format!("failed to bind orchestrator scaffold to {}", args.bind_addr))?;

    tracing::info!(bind_addr = %args.bind_addr, "orchestrator scaffold listening");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let _ = stream.write_all(b"catalyst continuum orchestrator scaffold\n");
            }
            Err(error) => {
                tracing::warn!(%error, "failed to accept incoming connection");
            }
        }
    }

    Ok(())
}

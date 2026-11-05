//! `sillytavern-rs-server`: run behind an HTTPS reverse proxy (see docker-compose.yml).
//!
//! Environment:
//!   ST_SYNC_DATA   data folder (default /data)       — holds sync.db and token
//!   ST_SYNC_ADDR   listen address (default 0.0.0.0:8787)
//!   ST_SYNC_TOKEN  fixed access token (optional; otherwise generated once)

use std::path::PathBuf;
use std::sync::Arc;

use sillytavern_rs_server::{Server, router};

#[tokio::main]
async fn main() {
    let data = PathBuf::from(std::env::var("ST_SYNC_DATA").unwrap_or_else(|_| "/data".into()));
    let addr = std::env::var("ST_SYNC_ADDR").unwrap_or_else(|_| "0.0.0.0:8787".into());
    let token = std::env::var("ST_SYNC_TOKEN").ok();

    let (server, generated) = match Server::open(&data, token) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Cannot start: {e}");
            std::process::exit(1);
        }
    };
    if generated {
        println!("New access token (enter it in the app): {}", server.token());
    }
    println!("Access token file: {}", data.join("token").display());

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Cannot listen on {addr}: {e}");
            std::process::exit(1);
        }
    };
    println!("SillyTavern RS sync server {} listening on {addr}", env!("CARGO_PKG_VERSION"));
    if let Err(e) = axum::serve(listener, router(Arc::new(server))).with_graceful_shutdown(shutdown()).await {
        eprintln!("Server error: {e}");
        std::process::exit(1);
    }
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

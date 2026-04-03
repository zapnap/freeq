//! AV SFU (Selective Forwarding Unit).
//!
//! Accepts MoQ connections via WebTransport (browser) and iroh QUIC (native).
//! Uses moq_relay::Cluster to route audio streams between all participants.
//! Serves the call web page via HTTP on the same port (TCP vs UDP).

#[cfg(feature = "av-native")]
pub async fn run_sfu(port: u16) -> anyhow::Result<()> {
    use moq_relay::{Cluster, ClusterConfig, Auth, AuthConfig};
    use std::sync::Arc;

    tracing::info!("Starting AV SFU on :{port}");

    // QUIC server (WebTransport + iroh MoQ)
    let mut server_config = moq_native::ServerConfig::default();
    server_config.bind = Some(format!("[::]:{port}").parse()?);
    server_config.backend = Some(moq_native::QuicBackend::Noq);
    server_config.max_streams = Some(moq_relay::DEFAULT_MAX_STREAMS);
    server_config.tls.generate = vec!["localhost".to_string()];

    let mut client_config = moq_native::ClientConfig::default();
    client_config.max_streams = Some(moq_relay::DEFAULT_MAX_STREAMS);

    let mut server = server_config.init()?;
    let client = client_config.init()?;
    let auth = Auth::new(AuthConfig::default()).await?;

    let tls_info = server.tls_info();

    // Cluster routes broadcasts between sessions
    let cluster = Cluster::new(ClusterConfig::default(), client);
    let cluster_handle = cluster.clone();
    tokio::spawn(async move {
        if let Err(e) = cluster_handle.run().await {
            tracing::error!("SFU cluster failed: {e}");
        }
    });

    // HTTP server (serves call page) on same port number (TCP, not UDP)
    let tls_for_http = tls_info.clone();
    tokio::spawn(async move {
        let app = axum::Router::new()
            .route("/certificate.sha256", axum::routing::get(move || {
                let info = tls_for_http.read().expect("tls lock");
                let fp = info.fingerprints.first().cloned().unwrap_or_default();
                async move { fp }
            }))
            .fallback(axum::routing::get(serve_static));

        let addr = format!("[::]:{port}");
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                tracing::info!("AV SFU HTTP on :{port} (call page)");
                axum::serve(listener, app).await.ok();
            }
            Err(e) => tracing::warn!("SFU HTTP bind failed on {addr}: {e}"),
        }
    });

    tracing::info!("AV SFU QUIC on :{port} (WebTransport + MoQ)");

    // Accept loop — handle MoQ sessions
    let mut conn_id: u64 = 0;
    while let Some(request) = server.accept().await {
        conn_id += 1;
        let cluster = cluster.clone();
        let auth = auth.clone();
        let id = conn_id;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(id, request, cluster, auth).await {
                tracing::debug!(conn = id, "SFU session ended: {e}");
            }
        });
    }

    Ok(())
}

#[cfg(feature = "av-native")]
async fn handle_connection(
    id: u64,
    request: moq_native::Request,
    cluster: moq_relay::Cluster,
    auth: moq_relay::Auth,
) -> anyhow::Result<()> {
    use moq_relay::AuthParams;

    let transport = request.transport();
    let params = match request.url() {
        Some(url) => AuthParams::from_url(url),
        None => AuthParams::default(),
    };

    let token = auth.verify(&params)?;
    let publish = cluster.publisher(&token);
    let subscribe = cluster.subscriber(&token);
    let _registration = cluster.register(&token);

    tracing::info!(conn = id, %transport, "SFU: client connected");

    let mut request = request;
    if let Some(p) = publish {
        request = request.with_consume(p);
    }
    if let Some(s) = subscribe {
        request = request.with_publish(s);
    }
    let session = request.ok().await?;

    tracing::info!(conn = id, "SFU: session active");
    session.closed().await;
    tracing::info!(conn = id, "SFU: session closed");

    Ok(())
}

// ── Static file serving ────────────────────────────────────────────

#[cfg(feature = "av-native")]
async fn serve_static(
    uri: axum::http::Uri,
) -> impl axum::response::IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "call.html" } else { path };

    // Serve from compiled-in static files
    let content = match path {
        "call.html" | "" => Some((include_str!("../static/av/call.html"), "text/html")),
        "index.html" => Some((include_str!("../static/av/index.html"), "text/html")),
        "publish.html" => Some((include_str!("../static/av/publish.html"), "text/html")),
        _ => None,
    };

    if let Some((body, mime)) = content {
        return (
            axum::http::StatusCode::OK,
            [("content-type", format!("{mime}; charset=utf-8"))],
            body.to_string(),
        ).into_response();
    }

    // Serve JS assets
    let js_files: &[(&str, &str)] = &[
        ("assets/watch-CQEo0ml-.js", include_str!("../static/av/assets/watch-CQEo0ml-.js")),
        ("assets/publish-0_tfMLVg.js", include_str!("../static/av/assets/publish-0_tfMLVg.js")),
        ("assets/time-Do1uKez-.js", include_str!("../static/av/assets/time-Do1uKez-.js")),
        ("assets/main-DGBFe0O7-CIZu5tmC.js", include_str!("../static/av/assets/main-DGBFe0O7-CIZu5tmC.js")),
        ("assets/main-DGBFe0O7-DQ8if_La.js", include_str!("../static/av/assets/main-DGBFe0O7-DQ8if_La.js")),
        ("assets/libav-opus-af-BlMWboA7-B4GfDr9_.js", include_str!("../static/av/assets/libav-opus-af-BlMWboA7-B4GfDr9_.js")),
        ("assets/libav-opus-af-BlMWboA7-CFTeN5TA.js", include_str!("../static/av/assets/libav-opus-af-BlMWboA7-CFTeN5TA.js")),
    ];

    for (name, body) in js_files {
        if path == *name {
            return (
                axum::http::StatusCode::OK,
                [("content-type", "application/javascript; charset=utf-8".to_string())],
                body.to_string(),
            ).into_response();
        }
    }

    (axum::http::StatusCode::NOT_FOUND, [("content-type", "text/plain".to_string())], "not found".to_string()).into_response()
}

use axum::response::IntoResponse;

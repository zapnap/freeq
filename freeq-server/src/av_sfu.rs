//! AV SFU (Selective Forwarding Unit) for browser WebTransport audio.
//!
//! Accepts WebTransport/MoQ connections from browsers on a dedicated port.
//! Uses moq_relay::Cluster to route audio streams between participants.
//! Bridges browser clients into iroh-live Rooms via the Cluster.

#[cfg(feature = "av-native")]
pub async fn run_sfu(port: u16) -> anyhow::Result<()> {
    use moq_relay::{Cluster, ClusterConfig, Auth, AuthConfig};

    tracing::info!("Starting AV SFU on :{port}");

    // Server config — accepts WebTransport connections
    let mut server_config = moq_native::ServerConfig::default();
    server_config.bind = Some(format!("[::]:{port}").parse()?);
    server_config.backend = Some(moq_native::QuicBackend::Noq);
    server_config.max_streams = Some(moq_relay::DEFAULT_MAX_STREAMS);
    server_config.tls.generate = vec!["localhost".to_string()];

    // Client config for outbound connections
    let mut client_config = moq_native::ClientConfig::default();
    client_config.max_streams = Some(moq_relay::DEFAULT_MAX_STREAMS);

    let mut server = server_config.init()?;
    let client = client_config.init()?;

    // No auth for now (staging)
    let auth = Auth::new(AuthConfig::default()).await?;

    // Cluster acts as the SFU — routes streams between sessions
    let cluster = Cluster::new(ClusterConfig::default(), client);
    let cluster_handle = cluster.clone();
    tokio::spawn(async move {
        if let Err(e) = cluster_handle.run().await {
            tracing::error!("SFU cluster failed: {e}");
        }
    });

    let tls_info = server.tls_info();
    tracing::info!("AV SFU listening on :{port}");

    // Serve embedded web page via axum on a separate HTTP listener
    let http_app = axum::Router::new()
        .route("/", axum::routing::get(serve_index))
        .route("/certificate.sha256", axum::routing::get({
            let tls = tls_info.clone();
            move || {
                let info = tls.read().expect("tls lock");
                let fp = info.fingerprints.first().cloned().unwrap_or_default();
                async move { fp }
            }
        }))
        .route("/{*path}", axum::routing::get(serve_index));
    let http_listener = tokio::net::TcpListener::bind(format!("[::]:{port}")).await;
    if let Ok(listener) = http_listener {
        tokio::spawn(async move {
            axum::serve(listener, http_app).await.ok();
        });
    }

    // Accept loop — handle incoming WebTransport connections
    let mut conn_id: u64 = 0;
    while let Some(request) = server.accept().await {
        conn_id += 1;
        let cluster = cluster.clone();
        let auth = auth.clone();
        let id = conn_id;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(id, request, cluster, auth).await {
                tracing::debug!(conn = id, "SFU connection ended: {e}");
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

    tracing::info!(conn = id, %transport, "SFU: browser connected");

    let mut request = request;
    if let Some(publish) = publish {
        request = request.with_consume(publish);
    }
    if let Some(subscribe) = subscribe {
        request = request.with_publish(subscribe);
    }
    let session = request.ok().await?;

    tracing::info!(conn = id, "SFU: session established");
    session.closed().await;
    tracing::info!(conn = id, "SFU: session closed");

    Ok(())
}

#[cfg(feature = "av-native")]
async fn serve_index() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        include_str!("av_sfu_page.html"),
    )
}

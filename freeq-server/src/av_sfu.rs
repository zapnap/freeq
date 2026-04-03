//! AV SFU (Selective Forwarding Unit).
//!
//! Accepts MoQ connections via WebTransport (browser) and iroh QUIC (native).
//! Uses moq_relay::Cluster to route audio streams between all participants.
//! Binds QUIC (UDP) on the web server's port. Call page served via web.rs.

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
    let mut auth_config = AuthConfig::default();
    auth_config.public = Some("/".to_string()); // All paths public (no auth for staging)
    let auth = Auth::new(auth_config).await?;

    // Cluster routes broadcasts between sessions
    let cluster = Cluster::new(ClusterConfig::default(), client);
    let cluster_handle = cluster.clone();
    tokio::spawn(async move {
        if let Err(e) = cluster_handle.run().await {
            tracing::error!("SFU cluster failed: {e}");
        }
    });

    // Call page is served through the main web server (see web.rs /av/* routes).
    // SFU only handles QUIC (UDP) on this port.
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
    let _ = session.closed().await;
    tracing::info!(conn = id, "SFU: session closed");

    Ok(())
}


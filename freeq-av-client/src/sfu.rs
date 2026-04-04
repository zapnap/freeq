//! SFU client — connect to freeq AV SFU via MoQ.
//!
//! Publishes mic audio as a MoQ broadcast and subscribes to other participants.
//! Uses the same protocol as the browser (moq-publish/moq-watch web components).

use anyhow::Result;
use iroh_live::media::{
    audio_backend::AudioBackend,
    codec::AudioCodec,
    format::AudioPreset,
    publish::LocalBroadcast,
};

/// Connect to the SFU, publish mic audio, and subscribe to other participants.
pub async fn run_sfu(sfu_url: &str, session: &str, nick: &str) -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let broadcast_name = format!("{session}/{nick}");
    tracing::info!(%sfu_url, %broadcast_name, "Connecting to SFU via MoQ");

    // Create MoQ client (QUIC, no TLS verification for self-signed SFU certs)
    let mut client_config = moq_native::ClientConfig::default();
    client_config.tls.disable_verify = Some(true);
    client_config.backend = Some(moq_native::QuicBackend::Noq);
    let client = client_config.init()?;

    // Set up audio capture
    let broadcast = LocalBroadcast::new();
    let audio_backend = AudioBackend::default();
    audio_backend.set_aec_enabled(false);

    let inputs = AudioBackend::list_inputs();
    let outputs = AudioBackend::list_outputs();
    println!("\n  Audio devices:");
    for d in &inputs { println!("    Input:  {}", d.name); }
    for d in &outputs { println!("    Output: {}", d.name); }

    let mic = audio_backend.default_input().await?;
    broadcast.audio().set(mic, AudioCodec::Opus, [AudioPreset::Hq])?;
    println!("  Microphone active (Opus).");

    // Create our publishing origin — announce our broadcast to the SFU
    let origin = moq_lite::Origin::produce();

    // Create a MoQ broadcast with an audio track
    let moq_broadcast = moq_lite::BroadcastProducer::new();
    // Enable dynamic track creation so the SFU can request tracks
    moq_broadcast.dynamic();

    // Publish our broadcast under the session/nick namespace
    origin.publish_broadcast(&broadcast_name, moq_broadcast.consume());
    tracing::info!(%broadcast_name, "Publishing audio broadcast");

    // Create subscription origin — receive other participants' broadcasts
    let sub_origin = moq_lite::Origin::produce();
    let mut sub_consumer = sub_origin.consume();

    // Connect to the SFU via WebSocket (works through any HTTP reverse proxy)
    let base: url::Url = sfu_url.parse()?;
    let url = base.join("/av/moq/")?;
    println!("  Connecting to {url}...");

    let session_handle = client
        .with_publish(origin.consume()) // server subscribes to our broadcasts (OriginConsumer)
        .with_consume(sub_origin)       // server announces others' broadcasts to us (OriginProducer)
        .connect(url)
        .await?;

    println!("  Connected to SFU!");
    println!("  Publishing as: {broadcast_name}");
    println!("  Press Ctrl+C to leave.\n");

    // Spawn a task to write audio frames to the MoQ broadcast
    let audio_broadcast = broadcast.clone();
    let bcast_name = broadcast_name.clone();
    tokio::spawn(async move {
        // The iroh-live LocalBroadcast handles encoding.
        // For now, keep it alive — the MoQ broadcast is separate.
        // TODO: pipe iroh-live encoded audio → MoQ track frames
        tracing::info!(%bcast_name, "Audio publish task running");
        // Keep the broadcast handle alive
        let _broadcast = audio_broadcast;
        tokio::signal::ctrl_c().await.ok();
    });

    // Watch for incoming broadcasts from other participants
    tokio::spawn(async move {
        while let Some((path, announce)) = sub_consumer.announced().await {
            match announce {
                Some(broadcast_consumer) => {
                    let path_str = path.to_string();
                    println!("  + Broadcast announced: {path_str}");
                    tracing::info!(%path_str, "Remote broadcast announced");

                    // TODO: subscribe to audio track, decode Opus, play through AudioBackend
                    let _bc = broadcast_consumer;
                }
                None => {
                    let path_str = path.to_string();
                    println!("  - Broadcast removed: {path_str}");
                    tracing::info!(%path_str, "Remote broadcast removed");
                }
            }
        }
        tracing::info!("Subscription stream ended");
    });

    // Wait for session to close or Ctrl+C
    tokio::select! {
        result = session_handle.closed() => {
            if let Err(e) = result {
                tracing::warn!("SFU session closed: {e}");
            }
            println!("  Session ended.");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n  Leaving...");
        }
    }

    Ok(())
}

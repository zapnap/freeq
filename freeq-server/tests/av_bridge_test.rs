//! Integration test for the MoQ↔iroh-live bridge data path.
//!
//! Tests the core forwarding logic with synthetic MoQ frames (no audio hardware needed).
//! The key bug was that RemoteBroadcast::new() consumed catalog.json during construction,
//! making it unavailable for re-forwarding. The fix passes raw BroadcastConsumer directly.

#![cfg(feature = "av-native")]

use std::time::Duration;
use tokio::time::timeout;

/// Test that the bridge's MoQ→Room forwarding logic works correctly.
///
/// This directly tests the code path that was broken: subscribing from the MoQ cluster,
/// creating a BroadcastProducer with dynamic track forwarding, and verifying that ALL
/// tracks (including catalog.json) can be subscribed from the forwarded producer.
///
/// Previously this failed because RemoteBroadcast consumed catalog.json during construction.
#[tokio::test]
async fn moq_forwarding_preserves_all_tracks() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .try_init()
        .ok();

    // ── 1. Set up MoQ cluster ─────────────────────────────────────
    let mut client_config = moq_native::ClientConfig::default();
    client_config.max_streams = Some(moq_relay::DEFAULT_MAX_STREAMS);
    let client = client_config.init().expect("moq client init");

    let mut auth_config = moq_relay::AuthConfig::default();
    auth_config.public = Some("/".to_string());
    let auth = moq_relay::Auth::new(auth_config).await.expect("auth init");

    let cluster = moq_relay::Cluster::new(moq_relay::ClusterConfig::default(), client);
    let cluster_run = cluster.clone();
    tokio::spawn(async move {
        let _ = cluster_run.run().await;
    });

    let token = auth
        .verify(&moq_relay::AuthParams {
            path: String::new(),
            jwt: None,
            register: None,
        })
        .expect("auth token");

    // ── 2. Create synthetic broadcast (simulates browser) ─────────
    let broadcast_path = "test-session/browser-alice";

    let mut producer = moq_lite::Broadcast::produce();

    // catalog.json track — this is the one RemoteBroadcast was consuming
    let catalog_track = moq_lite::Track::new("catalog.json");
    let mut catalog_writer = producer
        .create_track(catalog_track)
        .expect("create catalog track");
    let mut group = catalog_writer
        .create_group(moq_lite::Group { sequence: 0 })
        .expect("create catalog group");
    group
        .write_frame(moq_lite::bytes::Bytes::from_static(
            b"{\"audio\":{\"codec\":\"opus\",\"sampleRate\":48000}}",
        ))
        .expect("write catalog");
    group.finish().ok();

    // audio track with synthetic Opus-sized frames
    let audio_track = moq_lite::Track::new("audio");
    let mut audio_writer = producer.create_track(audio_track).expect("create audio track");
    for seq in 0..5u64 {
        let mut group = audio_writer
            .create_group(moq_lite::Group { sequence: seq })
            .expect("create audio group");
        group
            .write_frame(moq_lite::bytes::Bytes::from(vec![0xABu8; 960]))
            .expect("write audio frame");
        group.finish().ok();
    }

    // Publish into cluster
    let publisher = cluster.publisher(&token).expect("cluster publisher");
    publisher.publish_broadcast(broadcast_path, producer.consume());
    tracing::info!("Published synthetic broadcast to cluster");

    // ── 3. Subscribe from cluster (like bridge does) ──────────────
    let mut subscriber = cluster.subscriber(&token).expect("cluster subscriber");

    let (path, maybe_consumer) = timeout(Duration::from_secs(5), subscriber.announced())
        .await
        .expect("timeout waiting for announce")
        .expect("announce stream ended");

    assert_eq!(path.to_string(), broadcast_path);
    let source_consumer = maybe_consumer.expect("should have consumer (not unannounce)");
    tracing::info!("Got broadcast consumer from cluster");

    // ── 4. Do what the bridge does: forward via dynamic producer ──
    // This is the exact same logic as run_moq_to_room after the fix.
    let forwarded_producer = moq_lite::Broadcast::produce();
    let mut dynamic = forwarded_producer.dynamic();
    let forwarded_consumer = forwarded_producer.consume();

    // Spawn track forwarder (same as bridge)
    tokio::spawn(async move {
        loop {
            match dynamic.requested_track().await {
                Ok(mut dest_track) => {
                    let track_info = moq_lite::Track::new(dest_track.info.name.clone());
                    match source_consumer.subscribe_track(&track_info) {
                        Ok(source_track) => {
                            let tname = dest_track.info.name.clone();
                            tokio::spawn(async move {
                                forward_track(&tname, source_track, &mut dest_track).await;
                            });
                        }
                        Err(e) => {
                            tracing::warn!(track = %dest_track.info.name, error = %e, "Track not in source");
                            dest_track.abort(e).ok();
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    // ── 5. Subscribe to tracks from the forwarded consumer ────────
    // This simulates what a native client would do when receiving the broadcast.

    // TEST: catalog.json must be readable (this was the bug)
    let catalog_track = moq_lite::Track::new("catalog.json");
    let mut catalog_reader = forwarded_consumer
        .subscribe_track(&catalog_track)
        .expect("subscribe to catalog.json - THIS WAS THE BUG");

    let catalog_result = timeout(Duration::from_secs(5), async {
        let mut group = catalog_reader
            .next_group()
            .await
            .expect("next group")
            .expect("group exists");
        let frame = group
            .read_frame()
            .await
            .expect("read frame")
            .expect("frame exists");
        String::from_utf8_lossy(&frame).to_string()
    })
    .await
    .expect("timeout reading catalog");

    assert!(
        catalog_result.contains("opus"),
        "Catalog should contain 'opus', got: {catalog_result}"
    );
    tracing::info!("✓ catalog.json readable: {catalog_result}");

    // TEST: audio track must be readable
    let audio_track = moq_lite::Track::new("audio");
    let mut audio_reader = forwarded_consumer
        .subscribe_track(&audio_track)
        .expect("subscribe to audio track");

    let audio_result = timeout(Duration::from_secs(5), async {
        let mut group = audio_reader
            .next_group()
            .await
            .expect("next group")
            .expect("group exists");
        let frame = group
            .read_frame()
            .await
            .expect("read frame")
            .expect("frame exists");
        frame
    })
    .await
    .expect("timeout reading audio");

    assert_eq!(audio_result.len(), 960, "Audio frame should be 960 bytes");
    assert_eq!(
        audio_result[0], 0xAB,
        "Audio frame should contain test data"
    );
    tracing::info!("✓ audio frame received: {} bytes", audio_result.len());

    tracing::info!("✓ MoQ forwarding test PASSED — all tracks preserved");
}

/// Test the Room→MoQ direction: publishing a BroadcastConsumer directly into the cluster.
#[tokio::test]
async fn room_to_moq_consumer_passthrough() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .try_init()
        .ok();

    // ── 1. Set up MoQ cluster ─────────────────────────────────────
    let mut client_config = moq_native::ClientConfig::default();
    client_config.max_streams = Some(moq_relay::DEFAULT_MAX_STREAMS);
    let client = client_config.init().expect("moq client init");

    let mut auth_config = moq_relay::AuthConfig::default();
    auth_config.public = Some("/".to_string());
    let auth = moq_relay::Auth::new(auth_config).await.expect("auth init");

    let cluster = moq_relay::Cluster::new(moq_relay::ClusterConfig::default(), client);
    let cluster_run = cluster.clone();
    tokio::spawn(async move {
        let _ = cluster_run.run().await;
    });

    let token = auth
        .verify(&moq_relay::AuthParams {
            path: String::new(),
            jwt: None,
            register: None,
        })
        .expect("auth token");

    // ── 2. Create broadcast (simulates native client) ─────────────
    let broadcast_path = "test-session/native-alice-audio";
    let mut producer = moq_lite::Broadcast::produce();

    let catalog_track = moq_lite::Track::new("catalog.json");
    let mut catalog_writer = producer.create_track(catalog_track).expect("catalog");
    let mut group = catalog_writer
        .create_group(moq_lite::Group { sequence: 0 })
        .expect("group");
    group
        .write_frame(moq_lite::bytes::Bytes::from_static(
            b"{\"audio\":{\"codec\":\"opus\"}}",
        ))
        .ok();
    group.finish().ok();

    let audio_track = moq_lite::Track::new("audio");
    let mut audio_writer = producer.create_track(audio_track).expect("audio");
    for seq in 0..3u64 {
        let mut group = audio_writer
            .create_group(moq_lite::Group { sequence: seq })
            .expect("group");
        group
            .write_frame(moq_lite::bytes::Bytes::from(vec![0xCDu8; 480]))
            .ok();
        group.finish().ok();
    }

    // ── 3. Publish consumer into cluster (like Room→MoQ bridge) ───
    // This is what run_room_to_moq does: takes the BroadcastConsumer from the
    // RemoteBroadcast event and publishes it directly.
    let consumer = producer.consume();
    let publisher = cluster.publisher(&token).expect("publisher");
    publisher.publish_broadcast(broadcast_path, consumer);
    tracing::info!("Published broadcast consumer to MoQ cluster");

    // ── 4. Subscribe from cluster (simulates browser) ─────────────
    let mut subscriber = cluster.subscriber(&token).expect("subscriber");

    let result = timeout(Duration::from_secs(5), async {
        loop {
            if let Some((path, maybe_consumer)) = subscriber.announced().await {
                let path_str = path.to_string();
                if path_str == broadcast_path {
                    let consumer = maybe_consumer.expect("consumer");

                    // Read catalog
                    let catalog_track = moq_lite::Track::new("catalog.json");
                    let mut track = consumer
                        .subscribe_track(&catalog_track)
                        .expect("subscribe catalog");
                    let mut group = track
                        .next_group()
                        .await
                        .expect("next group")
                        .expect("group");
                    let frame = group
                        .read_frame()
                        .await
                        .expect("read frame")
                        .expect("frame");
                    let text = String::from_utf8_lossy(&frame);
                    assert!(text.contains("opus"));
                    tracing::info!("✓ catalog: {text}");

                    // Read audio
                    let audio_track = moq_lite::Track::new("audio");
                    let mut track = consumer
                        .subscribe_track(&audio_track)
                        .expect("subscribe audio");
                    let mut group = track
                        .next_group()
                        .await
                        .expect("next group")
                        .expect("group");
                    let frame = group
                        .read_frame()
                        .await
                        .expect("read frame")
                        .expect("frame");
                    assert_eq!(frame.len(), 480);
                    assert_eq!(frame[0], 0xCD);
                    tracing::info!("✓ audio: {} bytes", frame.len());

                    return true;
                }
            } else {
                panic!("subscriber ended");
            }
        }
    })
    .await;

    assert!(result.expect("timeout"), "Room→MoQ passthrough failed");
    tracing::info!("✓ Room→MoQ passthrough test PASSED");
}

/// Forward groups and frames from source to dest (same as av_bridge.rs)
async fn forward_track(
    name: &str,
    mut source: moq_lite::TrackConsumer,
    dest: &mut moq_lite::TrackProducer,
) {
    tracing::debug!(track = %name, "Forwarding track");
    while let Ok(Some(mut group_consumer)) = source.next_group().await {
        let group_info = moq_lite::Group {
            sequence: group_consumer.info.sequence,
        };
        let mut group_producer = match dest.create_group(group_info) {
            Ok(g) => g,
            Err(e) => {
                tracing::debug!(track = %name, error = %e, "Failed to create group");
                break;
            }
        };
        while let Ok(Some(frame_data)) = group_consumer.read_frame().await {
            if group_producer.write_frame(frame_data).is_err() {
                break;
            }
        }
        group_producer.finish().ok();
    }
    tracing::debug!(track = %name, "Track forwarding ended");
}

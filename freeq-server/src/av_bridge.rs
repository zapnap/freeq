//! Bridge between MoQ SFU cluster and iroh-live Rooms.
//!
//! When a browser publishes audio to the MoQ cluster, this bridge takes
//! that audio and republishes it into the iroh-live Room so native clients
//! can hear it. And vice versa: native client audio from the Room is
//! published into the MoQ cluster for browser clients.
//!
//! Both systems use the same underlying format (Opus in hang/MoQ broadcasts),
//! so no transcoding is needed — we just pipe BroadcastConsumers between them.

#[cfg(feature = "av-native")]
use std::sync::Arc;

/// Start the bidirectional bridge between an MoQ cluster and an iroh-live Room.
///
/// This spawns a background task that:
/// 1. Subscribes to MoQ cluster broadcasts and republishes them into the Room
/// 2. Listens for Room broadcasts and republishes them into the MoQ cluster
///
/// Returns a handle that keeps the bridge alive. Drop it to stop bridging.
#[cfg(feature = "av-native")]
pub fn start_bridge(
    session_id: String,
    cluster: moq_relay::Cluster,
    auth: moq_relay::Auth,
    room_handle: iroh_live::rooms::RoomHandle,
    room_events: iroh_live::rooms::RoomEvents,
) -> BridgeHandle {
    let cancel = tokio_util::sync::CancellationToken::new();

    // MoQ → Room: take browser audio from cluster, publish into Room
    let cancel2 = cancel.clone();
    let cluster2 = cluster.clone();
    let auth2 = auth.clone();
    let room2 = room_handle.clone();
    let sid = session_id.clone();
    let moq_to_room = tokio::spawn(async move {
        if let Err(e) = run_moq_to_room(sid, cluster2, auth2, room2, cancel2).await {
            tracing::warn!(error = %e, "MoQ→Room bridge ended");
        }
    });

    // Room → MoQ: take native audio from Room, publish into cluster
    let cancel3 = cancel.clone();
    let cluster3 = cluster;
    let auth3 = auth;
    let sid2 = session_id;
    let room_to_moq = tokio::spawn(async move {
        if let Err(e) = run_room_to_moq(sid2, cluster3, auth3, room_events, cancel3).await {
            tracing::warn!(error = %e, "Room→MoQ bridge ended");
        }
    });

    BridgeHandle {
        cancel,
        _moq_to_room: moq_to_room,
        _room_to_moq: room_to_moq,
    }
}

#[cfg(feature = "av-native")]
pub struct BridgeHandle {
    cancel: tokio_util::sync::CancellationToken,
    _moq_to_room: tokio::task::JoinHandle<()>,
    _room_to_moq: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "av-native")]
impl Drop for BridgeHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// MoQ → Room direction.
///
/// Subscribe to the MoQ cluster as a consumer, watch for announced broadcasts
/// (browser participants publishing audio), and republish each into the iroh-live Room.
#[cfg(feature = "av-native")]
async fn run_moq_to_room(
    session_id: String,
    cluster: moq_relay::Cluster,
    auth: moq_relay::Auth,
    room_handle: iroh_live::rooms::RoomHandle,
    cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    use moq_relay::AuthParams;

    // Create an auth token for subscribing to the cluster
    let params = AuthParams {
        path: String::new(), // root path — see all broadcasts
        jwt: None,
        register: None,
    };
    let token = auth.verify(&params)?;

    let Some(mut subscriber) = cluster.subscriber(&token) else {
        anyhow::bail!("Cluster returned no subscriber");
    };

    tracing::info!(session = %session_id, "MoQ→Room bridge started");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            announce = subscriber.announced() => {
                let Some((path, maybe_consumer)) = announce else { break };
                let path_str = path.to_string();

                // Only bridge broadcasts for our session
                if !path_str.starts_with(&session_id) {
                    continue;
                }

                match maybe_consumer {
                    Some(consumer) => {
                        // Browser published a broadcast — republish into the Room.
                        // Create a LocalBroadcast from the MoQ consumer via RemoteBroadcast,
                        // then re-publish using the Room's MoQ transport.
                        tracing::info!(session = %session_id, broadcast = %path_str, "Bridging MoQ broadcast → Room");

                        // Use the Room's underlying Live instance to publish the consumer directly.
                        // RoomHandle.publish_producer wants a BroadcastProducer, but we have a Consumer.
                        // The underlying MoQ transport (iroh-moq) CAN publish consumers directly.
                        // For now, use the Live's publish method which also takes BroadcastConsumer
                        // indirectly through the broadcast producer pattern.

                        // Create a new broadcast, pipe the MoQ consumer's tracks through it
                        let broadcast = moq_lite::Broadcast::produce();
                        let broadcast_consumer = consumer;

                        // Spawn a forwarding task that bridges tracks from source to dest
                        let room = room_handle.clone();
                        let name = path_str.clone();
                        tokio::spawn(async move {
                            // Publish the producer side into the Room
                            if let Err(e) = room.publish_producer(&name, broadcast.clone()).await {
                                tracing::warn!(broadcast = %name, error = %e, "Failed to publish bridge broadcast to Room");
                                return;
                            }
                            tracing::info!(broadcast = %name, "Bridge broadcast published to Room");

                            // Use the dynamic handler to forward track requests from Room peers
                            // to the MoQ source broadcast
                            let mut dynamic = broadcast.dynamic();
                            while let Some(mut request) = dynamic.requested().await {
                                let track_name = request.track().name.clone();
                                match broadcast_consumer.subscribe_track(request.track()) {
                                    Ok(source_track) => {
                                        // Create a track producer and forward data
                                        let track_producer = request.produce();
                                        tokio::spawn(forward_track(track_name, source_track, track_producer));
                                    }
                                    Err(e) => {
                                        tracing::debug!(track = %track_name, error = %e, "Track not available from MoQ source");
                                        request.close(moq_lite::Error::NotFound);
                                    }
                                }
                            }
                        });
                    }
                    None => {
                        tracing::info!(session = %session_id, broadcast = %path_str, "MoQ broadcast unannounced");
                        // Broadcast removed — Room will handle cleanup when the producer is dropped
                    }
                }
            }
        }
    }

    tracing::info!(session = %session_id, "MoQ→Room bridge stopped");
    Ok(())
}

/// Forward a single track's data from a MoQ consumer to a producer.
#[cfg(feature = "av-native")]
async fn forward_track(
    name: String,
    mut source: moq_lite::TrackConsumer,
    mut dest: moq_lite::TrackProducer,
) {
    tracing::debug!(track = %name, "Forwarding track");
    loop {
        match source.read().await {
            Ok(Some(frame)) => {
                if let Err(e) = dest.write(frame) {
                    tracing::debug!(track = %name, error = %e, "Track write failed");
                    break;
                }
            }
            Ok(None) => {
                tracing::debug!(track = %name, "Track source ended");
                break;
            }
            Err(e) => {
                tracing::debug!(track = %name, error = %e, "Track read error");
                break;
            }
        }
    }
}

/// Room → MoQ direction.
///
/// Listen for RoomEvent::BroadcastSubscribed (native clients publishing audio)
/// and republish each broadcast into the MoQ cluster so browser clients can hear them.
#[cfg(feature = "av-native")]
async fn run_room_to_moq(
    session_id: String,
    cluster: moq_relay::Cluster,
    auth: moq_relay::Auth,
    mut room_events: iroh_live::rooms::RoomEvents,
    cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    use moq_relay::AuthParams;

    let params = AuthParams {
        path: String::new(),
        jwt: None,
        register: None,
    };
    let token = auth.verify(&params)?;

    let Some(publisher) = cluster.publisher(&token) else {
        anyhow::bail!("Cluster returned no publisher");
    };

    tracing::info!(session = %session_id, "Room→MoQ bridge started");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            event = room_events.recv() => {
                let Some(event) = event else { break };
                match event {
                    iroh_live::rooms::RoomEvent::BroadcastSubscribed { session, broadcast } => {
                        let remote_id = session.remote_id();
                        // Use the display name or remote ID as the broadcast path
                        let display_name = session.display_name()
                            .unwrap_or_else(|| remote_id.to_string());
                        let broadcast_path = format!("{session_id}/{display_name}");

                        tracing::info!(
                            session = %session_id,
                            peer = %remote_id,
                            broadcast = %broadcast_path,
                            "Bridging Room broadcast → MoQ"
                        );

                        // Get the BroadcastConsumer from the Room broadcast
                        let consumer = broadcast.consume();

                        // Publish into the MoQ cluster
                        publisher.publish_broadcast(&broadcast_path, consumer);
                        tracing::info!(broadcast = %broadcast_path, "Room broadcast published to MoQ cluster");
                    }
                    iroh_live::rooms::RoomEvent::PeerJoined { display_name, remote } => {
                        let name = display_name.as_deref().unwrap_or("unknown");
                        tracing::info!(session = %session_id, peer = %remote, name = %name, "Native peer joined Room");
                    }
                    iroh_live::rooms::RoomEvent::PeerLeft { remote } => {
                        tracing::info!(session = %session_id, peer = %remote, "Native peer left Room");
                    }
                    _ => {}
                }
            }
        }
    }

    tracing::info!(session = %session_id, "Room→MoQ bridge stopped");
    Ok(())
}

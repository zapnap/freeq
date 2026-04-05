//! Bridge between MoQ SFU cluster and iroh-live Rooms.
//!
//! When a browser publishes audio to the MoQ cluster, this bridge takes
//! that audio and republishes it into the iroh-live Room so native clients
//! can hear it. And vice versa: native client audio from the Room is
//! published into the MoQ cluster for browser clients.
//!
//! Both systems use the same underlying format (Opus in hang/MoQ broadcasts),
//! so no transcoding is needed — we pipe BroadcastConsumers between them.

/// Start the bidirectional bridge between an MoQ cluster and an iroh-live Room.
///
/// Spawns background tasks that:
/// 1. Subscribe to MoQ cluster broadcasts → republish into Room
/// 2. Listen for Room broadcasts → republish into MoQ cluster
///
/// Drop the returned handle to stop the bridge.
#[cfg(feature = "av-native")]
pub fn start_bridge(
    session_id: String,
    cluster: moq_relay::Cluster,
    auth: moq_relay::Auth,
    room_handle: iroh_live::rooms::RoomHandle,
    room_events: iroh_live::rooms::RoomEvents,
) -> BridgeHandle {
    let shutdown = tokio::sync::watch::channel(false);

    // MoQ → Room: browser audio into iroh-live Room
    let sid = session_id.clone();
    let cluster2 = cluster.clone();
    let auth2 = auth.clone();
    let room2 = room_handle.clone();
    let mut rx1 = shutdown.1.clone();
    let moq_to_room = tokio::spawn(async move {
        if let Err(e) = run_moq_to_room(&sid, &cluster2, &auth2, &room2, &mut rx1).await {
            tracing::warn!(session = %sid, error = %e, "MoQ→Room bridge ended");
        }
    });

    // Room → MoQ: native audio into MoQ cluster
    let sid2 = session_id;
    let mut rx2 = shutdown.1.clone();
    let room_to_moq = tokio::spawn(async move {
        if let Err(e) = run_room_to_moq(&sid2, &cluster, &auth, room_events, &mut rx2).await {
            tracing::warn!(session = %sid2, error = %e, "Room→MoQ bridge ended");
        }
    });

    BridgeHandle {
        _shutdown: shutdown.0,
        _moq_to_room: moq_to_room,
        _room_to_moq: room_to_moq,
    }
}

#[cfg(feature = "av-native")]
pub struct BridgeHandle {
    _shutdown: tokio::sync::watch::Sender<bool>,
    _moq_to_room: tokio::task::JoinHandle<()>,
    _room_to_moq: tokio::task::JoinHandle<()>,
}

/// MoQ → Room: subscribe to MoQ cluster broadcasts, republish into iroh-live Room.
#[cfg(feature = "av-native")]
async fn run_moq_to_room(
    session_id: &str,
    cluster: &moq_relay::Cluster,
    auth: &moq_relay::Auth,
    room_handle: &iroh_live::rooms::RoomHandle,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let params = moq_relay::AuthParams {
        path: String::new(),
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
            _ = shutdown.changed() => break,
            announce = subscriber.announced() => {
                let Some((path, maybe_consumer)) = announce else { break };
                let path_str = path.to_string();

                // Only bridge broadcasts for our session
                if !path_str.starts_with(session_id) {
                    continue;
                }

                if let Some(consumer) = maybe_consumer {
                    tracing::info!(session = %session_id, broadcast = %path_str, "Bridging MoQ → Room");

                    // Wrap the MoQ consumer as a RemoteBroadcast to get the BroadcastProducer
                    // we need for the Room. RemoteBroadcast reads the hang catalog and
                    // re-exposes the broadcast.
                    let name = path_str.clone();
                    let room = room_handle.clone();
                    tokio::spawn(async move {
                        match iroh_live::media::subscribe::RemoteBroadcast::new(&name, consumer).await {
                            Ok(remote) => {
                                // Get the underlying consumer and create a producer wrapper
                                let inner_consumer = remote.consumer().clone();
                                // Create a fresh broadcast and pipe tracks through
                                let producer = moq_lite::Broadcast::produce();
                                let source = inner_consumer;

                                // Use dynamic track forwarding: when the Room peer requests a track,
                                // subscribe it from the source broadcast
                                let mut dynamic = producer.dynamic();
                                let room2 = room.clone();
                                let name2 = name.clone();

                                // Publish the producer into the Room
                                if let Err(e) = room2.publish_producer(&name2, producer.clone()).await {
                                    tracing::warn!(broadcast = %name, error = %e, "Failed to publish to Room");
                                    return;
                                }
                                tracing::info!(broadcast = %name, "Published MoQ broadcast to Room");

                                // Forward track requests
                                loop {
                                    match dynamic.requested_track().await {
                                        Ok(mut dest_track) => {
                                            let track_info = moq_lite::Track::new(dest_track.info.name.clone());
                                            match source.subscribe_track(&track_info) {
                                                Ok(source_track) => {
                                                    let tname = dest_track.info.name.clone();
                                                    tokio::spawn(async move {
                                                        forward_track(&tname, source_track, &mut dest_track).await;
                                                    });
                                                }
                                                Err(e) => {
                                                    tracing::debug!(track = %dest_track.info.name, error = %e, "Track not in source");
                                                    dest_track.abort(e).ok();
                                                }
                                            }
                                        }
                                        Err(_) => break, // Dynamic closed
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(broadcast = %name, error = %e, "Failed to create RemoteBroadcast for bridge");
                            }
                        }
                    });
                } else {
                    tracing::info!(session = %session_id, broadcast = %path_str, "MoQ broadcast unannounced");
                }
            }
        }
    }

    tracing::info!(session = %session_id, "MoQ→Room bridge stopped");
    Ok(())
}

/// Forward groups and frames from a source track to a destination track.
#[cfg(feature = "av-native")]
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
        // Forward all frames in this group
        while let Ok(Some(frame_data)) = group_consumer.read_frame().await {
            if group_producer.write_frame(frame_data).is_err() {
                break;
            }
        }
        group_producer.finish().ok();
    }
    tracing::debug!(track = %name, "Track forwarding ended");
}

/// Room → MoQ: listen for native client broadcasts, republish into MoQ cluster.
#[cfg(feature = "av-native")]
async fn run_room_to_moq(
    session_id: &str,
    cluster: &moq_relay::Cluster,
    auth: &moq_relay::Auth,
    mut room_events: iroh_live::rooms::RoomEvents,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let params = moq_relay::AuthParams {
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
            _ = shutdown.changed() => break,
            event = room_events.recv() => {
                let Some(event) = event else { break };
                match event {
                    iroh_live::rooms::RoomEvent::BroadcastSubscribed { session, broadcast } => {
                        let remote_id = session.remote_id();
                        let broadcast_name = broadcast.broadcast_name().to_string();
                        let broadcast_path = format!("{session_id}/{broadcast_name}");

                        tracing::info!(
                            session = %session_id,
                            peer = %remote_id,
                            broadcast = %broadcast_path,
                            "Bridging Room → MoQ"
                        );

                        // Get the BroadcastConsumer from the RemoteBroadcast and publish to cluster
                        let consumer = broadcast.consumer().clone();
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

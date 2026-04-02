//! Media backend for AV sessions using iroh-live.
//!
//! Creates iroh-live Rooms for each AV session. Participants join via
//! RoomTicket (native QUIC) or via the relay (browser WebTransport).

use std::collections::HashMap;
use std::sync::Arc;

use iroh_live::rooms::{Room, RoomTicket};
use iroh_live::Live;

/// Opaque ticket string for joining a media session.
pub type MediaTicket = String;

/// Abstraction over the real-time media transport.
pub trait MediaBackend: Send + Sync {
    /// Create a new media room and return a ticket for others to join.
    fn create_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<MediaTicket, String>> + Send + '_>>;

    /// Shut down a media room.
    fn close_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
}

/// iroh-live backed media transport.
///
/// Each AV session creates an iroh-live Room. The RoomTicket is returned
/// to clients so they can join via native QUIC (iroh) or browser WebTransport
/// (via the relay).
pub struct IrohLiveBackend {
    live: Arc<Live>,
    /// session_id → (Room, RoomTicket string)
    rooms: parking_lot::Mutex<HashMap<String, ActiveRoom>>,
}

struct ActiveRoom {
    ticket: String,
    // Room handle kept alive — dropping it leaves the room
    _handle: iroh_live::rooms::RoomHandle,
}

impl IrohLiveBackend {
    /// Create a new backend from an existing iroh endpoint.
    ///
    /// The endpoint should already have iroh-live's ALPN registered.
    /// Call this during server startup after the iroh endpoint is bound.
    pub async fn new(endpoint: iroh::Endpoint) -> Result<Self, String> {
        let live = Live::builder(endpoint)
            .with_router()
            .with_gossip()
            .spawn();
        tracing::info!(
            endpoint_id = %live.endpoint().id(),
            "iroh-live media backend initialized"
        );
        Ok(Self {
            live: Arc::new(live),
            rooms: parking_lot::Mutex::new(HashMap::new()),
        })
    }

    /// Get the Live instance (for relay integration).
    pub fn live(&self) -> &Live {
        &self.live
    }
}

impl MediaBackend for IrohLiveBackend {
    fn create_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<MediaTicket, String>> + Send + '_>> {
        let session_id = session_id.to_string();
        let live = self.live.clone();
        Box::pin(async move {
            // Generate a new room ticket (random gossip topic)
            let ticket = RoomTicket::generate();
            let room = Room::new(&live, ticket.clone())
                .await
                .map_err(|e| format!("Failed to create room: {e}"))?;

            let ticket_str = ticket.to_string();
            let (_events, handle) = room.split();

            // Store the handle to keep the room alive
            self.rooms.lock().insert(session_id.clone(), ActiveRoom {
                ticket: ticket_str.clone(),
                _handle: handle,
            });

            tracing::info!(
                session_id = %session_id,
                ticket = %ticket_str,
                "Created iroh-live media room"
            );

            Ok(ticket_str)
        })
    }

    fn close_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        let removed = self.rooms.lock().remove(session_id);
        let sid = session_id.to_string();
        Box::pin(async move {
            if removed.is_some() {
                // Dropping ActiveRoom drops the RoomHandle, which leaves the room
                tracing::info!(session_id = %sid, "Closed iroh-live media room");
            }
            Ok(())
        })
    }
}

/// Stub backend for testing without iroh-live.
pub struct StubMediaBackend {
    rooms: parking_lot::Mutex<HashMap<String, String>>,
}

impl StubMediaBackend {
    pub fn new() -> Self {
        Self {
            rooms: parking_lot::Mutex::new(HashMap::new()),
        }
    }
}

impl MediaBackend for StubMediaBackend {
    fn create_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<MediaTicket, String>> + Send + '_>> {
        let session_id = session_id.to_string();
        Box::pin(async move {
            let ticket = format!("stub://{session_id}");
            self.rooms.lock().insert(session_id, ticket.clone());
            Ok(ticket)
        })
    }

    fn close_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        self.rooms.lock().remove(session_id);
        Box::pin(async move { Ok(()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_create_and_close_room() {
        let backend = StubMediaBackend::new();
        let ticket = backend.create_room("test-session").await.unwrap();
        assert!(ticket.contains("test-session"));
        backend.close_room("test-session").await.unwrap();
    }
}

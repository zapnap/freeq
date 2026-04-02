//! Media backend for AV sessions.
//!
//! Abstracts the real-time media transport (iroh-live) behind a trait
//! so the session manager doesn't couple directly to iroh-live APIs.

use std::sync::Arc;

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
/// Uses the server's iroh endpoint to create Live sessions.
/// Participants connect directly via iroh QUIC using the ticket.
pub struct IrohLiveBackend {
    /// Room name → iroh-live ticket (for active rooms).
    rooms: parking_lot::Mutex<std::collections::HashMap<String, String>>,
}

impl IrohLiveBackend {
    pub fn new() -> Self {
        Self {
            rooms: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl MediaBackend for IrohLiveBackend {
    fn create_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<MediaTicket, String>> + Send + '_>> {
        let session_id = session_id.to_string();
        Box::pin(async move {
            // For now, return a placeholder ticket.
            // When iroh-live's Room API is stable, this will:
            // 1. Create a Live::from_endpoint(endpoint).with_router().spawn()
            // 2. Create a room broadcast
            // 3. Return the LiveTicket as a string
            //
            // The iroh-live API (from README):
            //   let live = Live::from_env().await?.with_router().spawn();
            //   let broadcast = LocalBroadcast::new();
            //   live.publish("session-{id}", &broadcast).await?;
            //   let ticket = LiveTicket::new(live.endpoint().addr(), "session-{id}");
            //
            // For now we store a stub ticket so the session flow works end-to-end.
            let ticket = format!("iroh-live://stub/{session_id}");
            self.rooms.lock().insert(session_id, ticket.clone());
            tracing::info!(ticket = %ticket, "Created iroh-live media room (stub)");
            Ok(ticket)
        })
    }

    fn close_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        let removed = self.rooms.lock().remove(session_id).is_some();
        let sid = session_id.to_string();
        Box::pin(async move {
            if removed {
                tracing::info!(session_id = %sid, "Closed iroh-live media room");
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_close_room() {
        let backend = IrohLiveBackend::new();
        let ticket = backend.create_room("test-session").await.unwrap();
        assert!(ticket.contains("test-session"));
        assert!(backend.rooms.lock().contains_key("test-session"));

        backend.close_room("test-session").await.unwrap();
        assert!(!backend.rooms.lock().contains_key("test-session"));
    }

    #[tokio::test]
    async fn close_nonexistent_room_is_ok() {
        let backend = IrohLiveBackend::new();
        assert!(backend.close_room("nope").await.is_ok());
    }
}

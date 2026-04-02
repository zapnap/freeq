//! Media backend for AV sessions.
//!
//! The actual browser audio uses WebRTC (peer-to-peer, signaled through IRC TAGMSG).
//! This module provides the server-side room tracking. When iroh-live is available
//! on a larger build server, the IrohLiveBackend can be re-enabled for native clients.

use std::collections::HashMap;

/// Opaque ticket string for joining a media session.
pub type MediaTicket = String;

/// Abstraction over the real-time media transport.
pub trait MediaBackend: Send + Sync {
    fn create_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<MediaTicket, String>> + Send + '_>>;

    fn close_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
}

/// Stub backend — tracks rooms without real media transport.
/// Browser audio uses WebRTC (signaled through IRC TAGMSG).
pub struct IrohLiveBackend {
    rooms: parking_lot::Mutex<HashMap<String, String>>,
}

impl IrohLiveBackend {
    pub fn new() -> Self {
        Self {
            rooms: parking_lot::Mutex::new(HashMap::new()),
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
            let ticket = format!("webrtc://{session_id}");
            self.rooms.lock().insert(session_id.clone(), ticket.clone());
            tracing::info!(session_id = %session_id, "Created AV room (WebRTC signaling)");
            Ok(ticket)
        })
    }

    fn close_room(
        &self,
        session_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        self.rooms.lock().remove(session_id);
        let sid = session_id.to_string();
        Box::pin(async move {
            tracing::info!(session_id = %sid, "Closed AV room");
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
        let ticket = backend.create_room("test").await.unwrap();
        assert!(ticket.contains("test"));
        backend.close_room("test").await.unwrap();
    }
}

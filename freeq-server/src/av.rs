//! AV Session subsystem — real-time voice/video/screen sharing.
//!
//! Sessions are first-class objects that live alongside IRC channels.
//! A session can be bound to a channel (most common) or ad-hoc (DM calls).
//!
//! Session control flows through IRC (TAGMSG with +freeq.at/av-* tags).
//! Media flows through iroh-live (separate QUIC path, not over IRC).
//! These are intentionally decoupled so the media backend is swappable.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// ULID-based session identifier.
pub type AvSessionId = String;

// ── Core types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AvSessionState {
    Active,
    Ended {
        ended_at: i64,
        ended_by: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvSession {
    pub id: AvSessionId,
    /// Channel this session is bound to (None for ad-hoc / DM calls).
    pub channel: Option<String>,
    /// DID of the user who created the session.
    pub created_by: String,
    /// Nick of the creator (for display).
    pub created_by_nick: String,
    pub created_at: i64,
    pub state: AvSessionState,
    /// DID → participant info.
    pub participants: HashMap<String, AvParticipant>,
    pub title: Option<String>,
    /// iroh-live connection ticket (opaque string, passed to clients).
    pub iroh_ticket: Option<String>,
    pub media_backend: MediaBackendType,
    pub recording_enabled: bool,
    pub max_participants: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvParticipant {
    pub did: String,
    pub nick: String,
    pub joined_at: i64,
    pub left_at: Option<i64>,
    pub role: ParticipantRole,
    pub tracks: Vec<TrackInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParticipantRole {
    Host,
    Speaker,
    Listener,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub kind: TrackKind,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Audio,
    Video,
    Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaBackendType {
    IrohLive,
}

impl Default for MediaBackendType {
    fn default() -> Self {
        Self::IrohLive
    }
}

// ── Artifacts (Phase 2) ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvArtifact {
    pub id: String,
    pub session_id: AvSessionId,
    pub kind: ArtifactKind,
    pub created_at: i64,
    /// DID of creator (None = system-generated).
    pub created_by: Option<String>,
    /// PDS blob CID or URL.
    pub content_ref: String,
    pub content_type: String,
    pub visibility: ArtifactVisibility,
    /// Human-readable title or filename.
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Transcript,
    Summary,
    Recording,
    Decisions,
    Tasks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactVisibility {
    Participants,
    Channel,
    Public,
}

impl Default for ArtifactVisibility {
    fn default() -> Self {
        Self::Participants
    }
}

// ── Session Manager ────────────────────────────────────────────────

/// Manages all active AV sessions. Lives in SharedState.
#[derive(Debug)]
pub struct AvSessionManager {
    /// Active + recently ended sessions (in-memory cache).
    pub sessions: HashMap<AvSessionId, AvSession>,
    /// Channel → active session ID (at most one active session per channel).
    pub channel_sessions: HashMap<String, AvSessionId>,
}

impl AvSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            channel_sessions: HashMap::new(),
        }
    }

    /// Create a new session. Returns the session or an error message.
    pub fn create_session(
        &mut self,
        channel: Option<&str>,
        creator_did: &str,
        creator_nick: &str,
        title: Option<&str>,
    ) -> Result<AvSession, String> {
        // Check: only one active session per channel.
        // If the existing session has no active participants (all left/disconnected),
        // auto-end it so a new session can start.
        if let Some(ch) = channel {
            if let Some(existing_id) = self.channel_sessions.get(&ch.to_lowercase()).cloned() {
                if let Some(existing) = self.sessions.get(&existing_id) {
                    if matches!(existing.state, AvSessionState::Active) {
                        let active_count = existing
                            .participants
                            .values()
                            .filter(|p| p.left_at.is_none())
                            .count();
                        if active_count > 0 {
                            return Err(format!(
                                "Channel {} already has an active session: {}",
                                ch, existing_id
                            ));
                        }
                        // No active participants — auto-end the stale session
                        tracing::info!(
                            session = %existing_id,
                            channel = %ch,
                            "Auto-ending stale session (0 active participants) to allow new session"
                        );
                        self.end_session_inner(&existing_id, Some(creator_did));
                    }
                }
            }
        }

        let id = ulid::Ulid::new().to_string();
        let now = chrono::Utc::now().timestamp();

        let mut participants = HashMap::new();
        participants.insert(
            creator_did.to_string(),
            AvParticipant {
                did: creator_did.to_string(),
                nick: creator_nick.to_string(),
                joined_at: now,
                left_at: None,
                role: ParticipantRole::Host,
                tracks: vec![],
            },
        );

        let session = AvSession {
            id: id.clone(),
            channel: channel.map(|s| s.to_string()),
            created_by: creator_did.to_string(),
            created_by_nick: creator_nick.to_string(),
            created_at: now,
            state: AvSessionState::Active,
            participants,
            title: title.map(|s| s.to_string()),
            iroh_ticket: None,
            media_backend: MediaBackendType::default(),
            recording_enabled: false,
            max_participants: None,
        };

        self.sessions.insert(id.clone(), session);
        if let Some(ch) = channel {
            self.channel_sessions
                .insert(ch.to_lowercase(), id.clone());
        }

        Ok(self.sessions.get(&id).unwrap().clone())
    }

    /// Join an existing session. Returns updated session or error.
    pub fn join_session(
        &mut self,
        session_id: &str,
        did: &str,
        nick: &str,
    ) -> Result<AvSession, String> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("Session {session_id} not found"))?;

        if !matches!(session.state, AvSessionState::Active) {
            return Err("Session has ended".to_string());
        }

        if let Some(max) = session.max_participants {
            let active = session
                .participants
                .values()
                .filter(|p| p.left_at.is_none())
                .count();
            if active >= max as usize {
                return Err("Session is full".to_string());
            }
        }

        let now = chrono::Utc::now().timestamp();

        // If already a participant who left, rejoin
        if let Some(p) = session.participants.get_mut(did) {
            p.left_at = None;
            p.joined_at = now;
        } else {
            session.participants.insert(
                did.to_string(),
                AvParticipant {
                    did: did.to_string(),
                    nick: nick.to_string(),
                    joined_at: now,
                    left_at: None,
                    role: ParticipantRole::Speaker,
                    tracks: vec![],
                },
            );
        }

        Ok(self.sessions.get(session_id).unwrap().clone())
    }

    /// Leave a session. Returns (session, should_end) — session ends if no active participants remain.
    pub fn leave_session(
        &mut self,
        session_id: &str,
        did: &str,
    ) -> Result<(AvSession, bool), String> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("Session {session_id} not found"))?;

        if let Some(p) = session.participants.get_mut(did) {
            p.left_at = Some(chrono::Utc::now().timestamp());
        }

        let active_count = session
            .participants
            .values()
            .filter(|p| p.left_at.is_none())
            .count();

        let should_end = active_count == 0;
        if should_end {
            self.end_session_inner(session_id, Some(did));
        }

        let session = self.sessions.get(session_id).unwrap().clone();
        Ok((session, should_end))
    }

    /// End a session (host or channel ops).
    pub fn end_session(
        &mut self,
        session_id: &str,
        ended_by: Option<&str>,
    ) -> Result<AvSession, String> {
        self.end_session_inner(session_id, ended_by);
        self.sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("Session {session_id} not found"))
    }

    fn end_session_inner(&mut self, session_id: &str, ended_by: Option<&str>) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            let now = chrono::Utc::now().timestamp();
            session.state = AvSessionState::Ended {
                ended_at: now,
                ended_by: ended_by.map(|s| s.to_string()),
            };
            // Mark all remaining participants as left
            for p in session.participants.values_mut() {
                if p.left_at.is_none() {
                    p.left_at = Some(now);
                }
            }
            // Remove from channel_sessions index
            if let Some(ch) = &session.channel {
                self.channel_sessions.remove(&ch.to_lowercase());
            }
        }
    }

    /// Get session by ID.
    pub fn get(&self, session_id: &str) -> Option<&AvSession> {
        self.sessions.get(session_id)
    }

    /// Get active session for a channel.
    pub fn active_session_for_channel(&self, channel: &str) -> Option<&AvSession> {
        let id = self.channel_sessions.get(&channel.to_lowercase())?;
        let session = self.sessions.get(id)?;
        if matches!(session.state, AvSessionState::Active) {
            Some(session)
        } else {
            None
        }
    }

    /// List all active sessions.
    pub fn active_sessions(&self) -> Vec<&AvSession> {
        self.sessions
            .values()
            .filter(|s| matches!(s.state, AvSessionState::Active))
            .collect()
    }

    /// Get active participant count for a session.
    pub fn active_participant_count(&self, session_id: &str) -> usize {
        self.sessions
            .get(session_id)
            .map(|s| s.participants.values().filter(|p| p.left_at.is_none()).count())
            .unwrap_or(0)
    }

    /// Check if a DID can end a session (must be host or channel op).
    pub fn can_end_session(&self, session_id: &str, did: &str) -> bool {
        self.sessions
            .get(session_id)
            .map(|s| {
                s.created_by == did
                    || s.participants
                        .get(did)
                        .map(|p| p.role == ParticipantRole::Host)
                        .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Apply a remote session event (from S2S federation).
    pub fn apply_remote_session_created(
        &mut self,
        id: &str,
        channel: Option<&str>,
        created_by_did: &str,
        created_by_nick: &str,
        title: Option<&str>,
        iroh_ticket: Option<&str>,
        created_at: i64,
    ) {
        if self.sessions.contains_key(id) {
            return; // Already known
        }

        let mut participants = HashMap::new();
        participants.insert(
            created_by_did.to_string(),
            AvParticipant {
                did: created_by_did.to_string(),
                nick: created_by_nick.to_string(),
                joined_at: created_at,
                left_at: None,
                role: ParticipantRole::Host,
                tracks: vec![],
            },
        );

        let session = AvSession {
            id: id.to_string(),
            channel: channel.map(|s| s.to_string()),
            created_by: created_by_did.to_string(),
            created_by_nick: created_by_nick.to_string(),
            created_at,
            state: AvSessionState::Active,
            participants,
            title: title.map(|s| s.to_string()),
            iroh_ticket: iroh_ticket.map(|s| s.to_string()),
            media_backend: MediaBackendType::default(),
            recording_enabled: false,
            max_participants: None,
        };

        if let Some(ch) = channel {
            self.channel_sessions
                .insert(ch.to_lowercase(), id.to_string());
        }
        self.sessions.insert(id.to_string(), session);
    }

    pub fn apply_remote_session_joined(&mut self, session_id: &str, did: &str, nick: &str) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            let now = chrono::Utc::now().timestamp();
            session
                .participants
                .entry(did.to_string())
                .and_modify(|p| {
                    p.left_at = None;
                    p.joined_at = now;
                })
                .or_insert_with(|| AvParticipant {
                    did: did.to_string(),
                    nick: nick.to_string(),
                    joined_at: now,
                    left_at: None,
                    role: ParticipantRole::Speaker,
                    tracks: vec![],
                });
        }
    }

    pub fn apply_remote_session_left(&mut self, session_id: &str, did: &str) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            if let Some(p) = session.participants.get_mut(did) {
                p.left_at = Some(chrono::Utc::now().timestamp());
            }
        }
    }

    pub fn apply_remote_session_ended(
        &mut self,
        session_id: &str,
        ended_by: Option<&str>,
    ) {
        self.end_session_inner(session_id, ended_by);
    }

    /// Leave all active sessions for a DID. Returns vec of (session_id, channel, nick, should_end).
    pub fn leave_all_for_did(&mut self, did: &str) -> Vec<(String, Option<String>, String, bool)> {
        let session_ids: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, s)| {
                matches!(s.state, AvSessionState::Active)
                    && s.participants
                        .get(did)
                        .map(|p| p.left_at.is_none())
                        .unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect();

        let mut results = Vec::new();
        for session_id in session_ids {
            let nick = self
                .sessions
                .get(&session_id)
                .and_then(|s| s.participants.get(did))
                .map(|p| p.nick.clone())
                .unwrap_or_default();
            match self.leave_session(&session_id, did) {
                Ok((session, should_end)) => {
                    let channel = session.channel.clone();
                    results.push((session_id, channel, nick, should_end));
                }
                Err(e) => {
                    tracing::warn!(session_id = %session_id, did = %did, error = %e, "Failed to leave AV session on disconnect");
                }
            }
        }
        results
    }

    /// Prune ended sessions older than `max_age_secs` from memory.
    pub fn prune_ended(&mut self, max_age_secs: i64) {
        let now = chrono::Utc::now().timestamp();
        self.sessions.retain(|_, s| match &s.state {
            AvSessionState::Active => true,
            AvSessionState::Ended { ended_at, .. } => now - ended_at < max_age_secs,
        });
    }
}

// DB persistence methods are in db.rs (needs access to private conn field).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_join_session() {
        let mut mgr = AvSessionManager::new();
        let session = mgr
            .create_session(Some("#test"), "did:plc:alice", "alice", Some("standup"))
            .unwrap();
        assert_eq!(session.created_by, "did:plc:alice");
        assert!(matches!(session.state, AvSessionState::Active));
        assert_eq!(session.participants.len(), 1);

        let id = session.id.clone();
        mgr.join_session(&id, "did:plc:bob", "bob").unwrap();
        let session = mgr.get(&id).unwrap();
        assert_eq!(session.participants.len(), 2);
        assert_eq!(mgr.active_participant_count(&id), 2);
    }

    #[test]
    fn one_session_per_channel() {
        let mut mgr = AvSessionManager::new();
        mgr.create_session(Some("#test"), "did:plc:alice", "alice", None)
            .unwrap();
        let err = mgr
            .create_session(Some("#test"), "did:plc:bob", "bob", None)
            .unwrap_err();
        assert!(err.contains("already has an active session"));
    }

    #[test]
    fn leave_and_auto_end() {
        let mut mgr = AvSessionManager::new();
        let session = mgr
            .create_session(Some("#test"), "did:plc:alice", "alice", None)
            .unwrap();
        let id = session.id.clone();

        let (_, should_end) = mgr.leave_session(&id, "did:plc:alice").unwrap();
        assert!(should_end);
        let session = mgr.get(&id).unwrap();
        assert!(matches!(session.state, AvSessionState::Ended { .. }));
    }

    #[test]
    fn end_session_marks_all_left() {
        let mut mgr = AvSessionManager::new();
        let session = mgr
            .create_session(Some("#test"), "did:plc:alice", "alice", None)
            .unwrap();
        let id = session.id.clone();
        mgr.join_session(&id, "did:plc:bob", "bob").unwrap();

        mgr.end_session(&id, Some("did:plc:alice")).unwrap();
        let session = mgr.get(&id).unwrap();
        assert!(session.participants.values().all(|p| p.left_at.is_some()));
    }

    #[test]
    fn rejoin_after_leaving() {
        let mut mgr = AvSessionManager::new();
        let session = mgr
            .create_session(Some("#test"), "did:plc:alice", "alice", None)
            .unwrap();
        let id = session.id.clone();
        mgr.join_session(&id, "did:plc:bob", "bob").unwrap();

        // Bob leaves (alice still in, so session doesn't end)
        mgr.leave_session(&id, "did:plc:bob").unwrap();
        assert_eq!(mgr.active_participant_count(&id), 1);

        // Bob rejoins
        mgr.join_session(&id, "did:plc:bob", "bob").unwrap();
        assert_eq!(mgr.active_participant_count(&id), 2);
    }

    #[test]
    fn channel_session_lookup() {
        let mut mgr = AvSessionManager::new();
        assert!(mgr.active_session_for_channel("#test").is_none());

        mgr.create_session(Some("#test"), "did:plc:alice", "alice", None)
            .unwrap();
        assert!(mgr.active_session_for_channel("#test").is_some());
        assert!(mgr.active_session_for_channel("#TEST").is_some()); // case insensitive
        assert!(mgr.active_session_for_channel("#other").is_none());
    }

    #[test]
    fn remote_session_lifecycle() {
        let mut mgr = AvSessionManager::new();
        mgr.apply_remote_session_created(
            "remote-1",
            Some("#collab"),
            "did:plc:remote",
            "remote_user",
            Some("sync"),
            Some("ticket-xyz"),
            1000,
        );
        assert!(mgr.active_session_for_channel("#collab").is_some());

        mgr.apply_remote_session_joined("remote-1", "did:plc:local", "local_user");
        assert_eq!(mgr.active_participant_count("remote-1"), 2);

        mgr.apply_remote_session_left("remote-1", "did:plc:local");
        assert_eq!(mgr.active_participant_count("remote-1"), 1);

        mgr.apply_remote_session_ended("remote-1", Some("did:plc:remote"));
        let session = mgr.get("remote-1").unwrap();
        assert!(matches!(session.state, AvSessionState::Ended { .. }));
        assert!(mgr.active_session_for_channel("#collab").is_none());
    }

    #[test]
    fn prune_ended_sessions() {
        let mut mgr = AvSessionManager::new();
        let session = mgr
            .create_session(Some("#old"), "did:plc:alice", "alice", None)
            .unwrap();
        let id = session.id.clone();
        mgr.leave_session(&id, "did:plc:alice").unwrap();

        // Session just ended — should not be pruned with max_age > 0
        mgr.prune_ended(3600);
        assert!(mgr.get(&id).is_some());

        // With max_age = 0, prune immediately
        mgr.prune_ended(0);
        assert!(mgr.get(&id).is_none());
    }
}

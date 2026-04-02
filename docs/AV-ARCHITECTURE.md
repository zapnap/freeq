# AV Architecture: Decoupled Real-Time Sessions

## TL;DR

Freeq treats voice/video/screen sharing as a **session subsystem** that lives alongside IRC channels. Session control (who's in the call, when it starts/ends) flows through IRC. Media transport (actual audio/video) flows through iroh-live over QUIC. These two concerns are completely decoupled — you can swap the media backend without touching session management, and you can build new clients that participate in sessions using only the IRC protocol for signaling or only iroh-live for media.

## Use Cases

### 1. Voice call from the web client

Alice opens freeq in her browser, joins `#standup`, clicks the phone icon. The server creates an AV session and an iroh-live Room. The session badge appears for everyone in the channel. Bob clicks "Join" — his browser opens a connection to the iroh-live relay via WebTransport and starts sending/receiving Opus audio. Alice and Bob talk. When they're done, Alice clicks "End" — the session closes, artifacts (summary, transcript) are generated and posted to the channel.

### 2. Native client joining a browser call

Charlie uses the freeq TUI client. He sees "Voice session active (2 participants)" in `#standup`. He runs `freeq-av server --url irc.freeq.at:6667 --channel '#standup' --join` which connects to the IRC server, joins the AV session, gets the iroh-live RoomTicket, and joins the same Room that Alice and Bob are in — directly over QUIC, no relay needed. All three hear each other.

### 3. Bot recording a session

A transcription bot connects to the IRC server, watches for AV session events via TAGMSG, joins the iroh-live Room as a listener, captures the audio stream, and pipes it to Whisper for real-time transcription. When the session ends, it posts the transcript as an artifact attached to the session. The bot never needs a browser or WebRTC — it uses iroh-live's Rust API directly.

### 4. Federated call across servers

freeq servers `irc.freeq.at` and `irc.zerosum.org` are federated via iroh S2S. When Alice starts a session on freeq.at, the session existence federates to zerosum.org via S2S (AvSessionCreated message). Users on zerosum.org see the active session badge. When they join, they connect to the iroh-live Room using the RoomTicket — iroh handles the peer-to-peer connection across servers. No central media server required.

---

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    freeq-server (Rust)                    │
│                                                          │
│  ┌─────────────────┐    ┌─────────────────────────────┐  │
│  │ AvSessionManager │    │ IrohLiveBackend             │  │
│  │                 │    │                             │  │
│  │ • sessions      │    │ • Live (iroh endpoint)      │  │
│  │ • participants  │◄──►│ • Rooms (per session)       │  │
│  │ • lifecycle     │    │ • RoomTickets               │  │
│  │ • DB persist    │    │                             │  │
│  └────────┬────────┘    └──────────┬──────────────────┘  │
│           │                        │                     │
│     IRC TAGMSG              iroh-live Room               │
│   (session control)        (media transport)             │
│           │                        │                     │
└───────────┼────────────────────────┼─────────────────────┘
            │                        │
    ┌───────┴───────┐       ┌────────┴────────┐
    │               │       │                 │
    │  IRC clients  │       │  Media clients  │
    │  (any IRC)    │       │  (iroh QUIC or  │
    │               │       │   WebTransport) │
    └───────────────┘       └─────────────────┘
```

### Two independent planes

**Control plane (IRC):** Session lifecycle messages flow through standard IRC TAGMSG with `+freeq.at/av-*` tags. Any IRC client can see session state, participant lists, and session events. The server manages all session logic — creating, joining, leaving, ending, authorization, persistence, S2S federation.

**Media plane (iroh-live):** Actual audio/video flows through iroh-live Rooms over QUIC. The server creates a Room when a session starts and distributes the RoomTicket to participants. Clients connect to the Room directly (peer-to-peer for native clients, via relay for browsers). The server doesn't touch media data.

### Why decoupled

A client can participate in the control plane without the media plane (see session state, manage participants) or the media plane without the control plane (join a Room directly by ticket for audio-only use). This means:

- **IRC-only clients** see sessions, participants, and artifacts without needing media support
- **Media-only tools** (recorders, transcribers, bridges) join Rooms without IRC
- **The media backend is swappable** — the `MediaBackend` trait abstracts room creation/teardown. iroh-live is the current implementation; others can be added without touching session management
- **Testing is independent** — session lifecycle can be tested without audio; audio can be tested without a server

---

## Technical Reference

### Session lifecycle (IRC protocol)

All session control uses TAGMSG with `+freeq.at/av-*` tags on the channel.

#### Start a session

```irc
@+freeq.at/av-start;+freeq.at/av-title=standup TAGMSG #channel
```

Server response (to creator):
```irc
:server NOTICE nick :AV session started: 01KN7XKRMWHQ...
:server NOTICE nick :AV ticket: roomaXYZ...
```

Server broadcast (to channel, TAGMSG for rich clients + NOTICE fallback):
```irc
@+freeq.at/av-state=started;+freeq.at/av-id=01KN7X...;+freeq.at/av-participants=1;+freeq.at/av-actor=nick :server TAGMSG #channel
```

#### Join a session

```irc
@+freeq.at/av-join;+freeq.at/av-id=01KN7X... TAGMSG #channel
```

or without an ID (joins the channel's active session):

```irc
@+freeq.at/av-join TAGMSG #channel
```

Server sends the RoomTicket to the joiner via NOTICE.

#### Leave / End

```irc
@+freeq.at/av-leave;+freeq.at/av-id=01KN7X... TAGMSG #channel
@+freeq.at/av-end;+freeq.at/av-id=01KN7X... TAGMSG #channel
```

Sessions auto-end when the last participant leaves.

### Media transport (iroh-live)

When `av-native` feature is enabled, the server creates real iroh-live Rooms:

```rust
let ticket = RoomTicket::generate();
let room = Room::new(&live, ticket.clone()).await?;
let (events, handle) = room.split();
// handle kept alive for session duration
// ticket.to_string() sent to participants
```

Native clients join by parsing the ticket and calling:

```rust
let ticket: RoomTicket = ticket_string.parse()?;
let room = Room::new(&live, ticket).await?;
// room.publish("audio", &broadcast) to send
// room.recv() events for incoming audio
```

Browser clients connect to the iroh-live relay at `:4443` via WebTransport. The relay bridges between the browser's WebTransport session and the iroh-live Room.

### REST API

```
GET  /api/v1/sessions                    Active sessions
GET  /api/v1/sessions/{id}               Session detail (includes iroh_ticket)
GET  /api/v1/sessions/{id}/artifacts     Session artifacts
POST /api/v1/sessions/{id}/artifacts     Create artifact
GET  /api/v1/channels/{name}/sessions    Active + recent sessions for channel
```

### S2S federation

Session lifecycle events federate via the existing S2S protocol:

```
AvSessionCreated  { session_id, channel, created_by_did, title, iroh_ticket, origin }
AvSessionJoined   { session_id, did, nick, origin }
AvSessionLeft     { session_id, did, origin }
AvSessionEnded    { session_id, ended_by, origin }
```

The `iroh_ticket` in `AvSessionCreated` allows users on remote servers to join the Room directly — iroh handles cross-network connectivity.

### Database schema

```sql
CREATE TABLE av_sessions (
    id TEXT PRIMARY KEY, channel TEXT, created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL, ended_at INTEGER, ended_by TEXT,
    title TEXT, iroh_ticket TEXT, backend TEXT, recording BOOLEAN,
    max_participants INTEGER
);

CREATE TABLE av_participants (
    session_id TEXT NOT NULL, did TEXT NOT NULL, nick TEXT NOT NULL,
    joined_at INTEGER NOT NULL, left_at INTEGER, role TEXT,
    PRIMARY KEY (session_id, did)
);

CREATE TABLE av_artifacts (
    id TEXT PRIMARY KEY, session_id TEXT NOT NULL, kind TEXT NOT NULL,
    created_at INTEGER NOT NULL, created_by TEXT, content_ref TEXT NOT NULL,
    content_type TEXT NOT NULL, visibility TEXT, title TEXT
);
```

### Artifact pipeline

When a session ends, the server can generate artifacts via pluggable backends:

```rust
pub trait TranscriptBackend: Send + Sync {
    fn transcribe(&self, audio_url: &str) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>>;
}

pub trait SummaryBackend: Send + Sync {
    fn summarize(&self, transcript: &str, context: &SummaryContext) -> Pin<Box<dyn Future<Output = Result<SummaryResult, String>> + Send + '_>>;
}
```

`SummaryResult` includes structured `decisions` and `action_items` in addition to the summary text. Artifacts are stored in the database and posted as channel notices.

### Building a plugin

To build something that participates in AV sessions, you have three integration points:

**IRC-level (session awareness):** Connect to the IRC server, watch for `+freeq.at/av-state` TAGMSGs. You'll see session created/joined/left/ended events with participant info. No media capability needed.

**REST-level (session management):** Use the REST API to list sessions, get details, create/list artifacts. Good for dashboards, analytics, or post-session processing.

**iroh-live-level (media participation):** Parse the RoomTicket from the session (via IRC NOTICE or REST API), create an iroh endpoint, join the Room. You can publish audio, subscribe to others' audio, or both. The `freeq-av-client` crate is a working example.

### Feature flags

The server compiles in two modes:

- **Default (no features):** Session management works, WebRTC signaling relays through TAGMSG, no iroh-live Rooms. Suitable for Miren/container deployment where disk is limited.
- **`av-native`:** Adds iroh-live + iroh-live-relay. Server creates real iroh-live Rooms and runs a WebTransport relay on `:4443`. Requires `libasound2-dev` and `cmake` on Linux. Suitable for VPS deployment.

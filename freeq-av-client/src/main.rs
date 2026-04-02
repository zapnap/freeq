//! freeq-av: Native audio client for freeq AV sessions.
//!
//! Joins an iroh-live room by ticket and publishes/subscribes audio.
//! Usage:
//!   freeq-av room                    # Create a room, print ticket
//!   freeq-av join <TICKET>           # Join an existing room
//!   freeq-av call <TICKET>           # Bidirectional call (create + join)

use anyhow::Result;
use clap::{Parser, Subcommand};
use iroh_live::{
    rooms::{Room, RoomEvent, RoomTicket},
    Live,
    media::{audio_backend::AudioBackend, codec::AudioCodec, format::AudioPreset, publish::LocalBroadcast},
};

#[derive(Parser)]
#[command(name = "freeq-av", about = "Native audio client for freeq AV sessions")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new room and print the ticket
    Room {
        /// Display name
        #[arg(short, long, default_value = "freeq-user")]
        name: String,
    },
    /// Join an existing room by ticket
    Join {
        /// Room ticket string
        ticket: String,
        /// Display name
        #[arg(short, long, default_value = "freeq-user")]
        name: String,
    },
    /// Connect to a freeq server, start/join an AV session, get iroh-live ticket
    Server {
        /// Server host:port (e.g. irc.freeq.at:8081)
        #[arg(short, long, default_value = "127.0.0.1:8081")]
        url: String,
        /// Channel to join
        #[arg(short, long, default_value = "#freeq")]
        channel: String,
        /// Display name / nick
        #[arg(short, long, default_value = "freeq-av-user")]
        name: String,
        /// Join existing session instead of starting a new one
        #[arg(long)]
        join: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("freeq_av=info".parse()?)
                .add_directive("iroh_live=info".parse()?)
                .add_directive("warn".parse()?),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Room { name } => run_room(name, None).await,
        Command::Join { ticket, name } => {
            let ticket: RoomTicket = ticket.parse()
                .map_err(|e| anyhow::anyhow!("Invalid room ticket: {e}"))?;
            run_room(name, Some(ticket)).await
        }
        Command::Server { url, channel, name, join } => run_server_session(&url, &channel, &name, join).await,
    }
}

async fn run_room(display_name: String, existing_ticket: Option<RoomTicket>) -> Result<()> {
    tracing::info!("Starting iroh-live audio client...");

    let live = Live::from_env().await?.with_router().with_gossip().spawn();
    tracing::info!(id = %live.endpoint().id(), "Endpoint ready");

    // Create or join room
    let ticket = existing_ticket.unwrap_or_else(RoomTicket::generate);
    let room = Room::new(&live, ticket.clone()).await?;
    let room_ticket = room.ticket();

    println!("\n  Room ticket: {room_ticket}\n");
    println!("  Share this ticket with others to join the call.");
    println!("  Or join from freeq web: the ticket appears in the session.\n");

    // Set display name
    let (mut events, handle) = room.split();
    handle.set_display_name(&display_name).await?;

    // Set up audio
    let broadcast = LocalBroadcast::new();
    let audio_backend = AudioBackend::default();

    // List available devices
    let inputs = AudioBackend::list_inputs();
    let outputs = AudioBackend::list_outputs();
    println!("  Audio devices:");
    for d in &inputs { println!("    Input:  {}", d.name); }
    for d in &outputs { println!("    Output: {}", d.name); }
    if inputs.is_empty() { println!("    WARNING: No input devices!"); }
    if outputs.is_empty() { println!("    WARNING: No output devices!"); }

    // Publish microphone
    let mic = audio_backend.default_input().await?;
    broadcast.audio().set(mic, AudioCodec::Opus, [AudioPreset::Hq])?;
    handle.publish("audio", &broadcast).await?;
    println!("  Microphone active (Opus). Press Ctrl+C to leave.\n");

    // Keep track handles alive so playback doesn't stop
    let mut _active_tracks: Vec<iroh_live::media::subscribe::MediaTracks> = Vec::new();

    // Event loop — handle incoming participants
    loop {
        match events.recv().await {
            Some(event) => match event {
                RoomEvent::PeerJoined { display_name, .. } => {
                    let name = display_name.as_deref().unwrap_or("unknown");
                    println!("  + {name} joined");
                    tracing::info!(%name, "Peer joined");
                }
                RoomEvent::BroadcastSubscribed { session, broadcast } => {
                    let tracks = broadcast.media(&audio_backend, Default::default()).await?;
                    if tracks.audio.is_some() {
                        let remote = session.remote_id();
                        tracing::info!(%remote, "Receiving audio");
                        println!("  ~ Receiving audio from {remote}");
                    }
                    // Keep the tracks handle alive — dropping it stops playback
                    _active_tracks.push(tracks);
                }
                RoomEvent::PeerLeft { remote } => {
                    println!("  - {remote} left");
                    tracing::info!(%remote, "Peer left");
                }
                RoomEvent::ChatReceived { message, .. } => {
                    println!("  [chat] {message:?}");
                }
                _ => {}
            },
            None => {
                tracing::info!("Room closed");
                break;
            }
        }
    }

    live.shutdown().await;
    Ok(())
}

/// Connect to a freeq server via IRC, start/join AV session, get iroh-live ticket.
async fn run_server_session(url: &str, channel: &str, nick: &str, join_existing: bool) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    // Parse host:port from URL
    let addr = url.trim_start_matches("ws://")
        .trim_start_matches("wss://")
        .trim_start_matches("irc://")
        .trim_end_matches("/irc")
        .trim_end_matches('/');
    let addr = if addr.contains(':') { addr.to_string() } else { format!("{addr}:6667") };

    println!("  Connecting to {addr}...");
    let stream = TcpStream::connect(&addr).await?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // IRC registration
    writer.write_all(format!("NICK {nick}\r\n").as_bytes()).await?;
    writer.write_all(format!("USER {nick} 0 * :freeq-av\r\n").as_bytes()).await?;

    let mut registered = false;
    let mut ticket: Option<String> = None;

    // Read until registered, then join channel and start session
    while let Some(line) = lines.next_line().await? {
        tracing::debug!("< {line}");

        // Respond to PING
        if line.starts_with("PING") {
            let pong = line.replace("PING", "PONG");
            writer.write_all(format!("{pong}\r\n").as_bytes()).await?;
            continue;
        }

        // 001 = RPL_WELCOME — registered
        if !registered && line.contains(" 001 ") {
            registered = true;
            println!("  Connected as {nick}");

            // Join channel
            writer.write_all(format!("JOIN {channel}\r\n").as_bytes()).await?;
            println!("  Joined {channel}");

            // Start or join AV session
            if join_existing {
                writer.write_all(format!("@+freeq.at/av-join TAGMSG {channel}\r\n").as_bytes()).await?;
                println!("  Joining AV session...");
            } else {
                writer.write_all(format!("@+freeq.at/av-start TAGMSG {channel}\r\n").as_bytes()).await?;
                println!("  Starting AV session...");
            }
        }

        // Look for AV ticket in NOTICE
        if line.contains("AV ticket:") {
            if let Some(t) = line.split("AV ticket: ").nth(1) {
                let t = t.trim();
                println!("  Got iroh-live ticket: {t}");
                ticket = Some(t.to_string());
                break;
            }
        }

        // Look for "AV session started" confirmation
        if line.contains("AV session started:") {
            println!("  Session created, waiting for ticket...");
        }
    }

    if let Some(ticket_str) = ticket {
        println!("  Joining iroh-live room...\n");
        let room_ticket: RoomTicket = ticket_str.parse()
            .map_err(|e| anyhow::anyhow!("Invalid room ticket from server: {e}"))?;

        // Keep the IRC connection alive in background
        let mut writer2 = writer;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let _ = writer2.write_all(b"PING :keepalive\r\n").await;
            }
        });

        // Join the iroh-live room
        run_room(nick.to_string(), Some(room_ticket)).await?;
    } else {
        println!("  No ticket received — server may not have iroh-live enabled.");
        println!("  You can still use 'freeq-av room' for standalone calls.");
    }

    Ok(())
}

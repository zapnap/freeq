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

    // Publish microphone
    let mic = audio_backend.default_input().await?;
    broadcast.audio().set(mic, AudioCodec::Opus, [AudioPreset::Hq])?;

    handle.publish("audio", &broadcast).await?;
    tracing::info!("Publishing audio from microphone");
    println!("  Microphone active. Press Ctrl+C to leave.\n");

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

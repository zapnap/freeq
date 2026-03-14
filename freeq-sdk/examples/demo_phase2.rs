//! Phase 2: Governable Agents — Interactive Demo
//!
//! Each step waits for the channel owner to say "next" before proceeding.
//!
//! Usage:
//!   cargo run --example demo_phase2 -- --server irc.freeq.at:6697 --tls --channel "#chad-dev"

use anyhow::Result;
use clap::Parser;
use freeq_sdk::auth::KeySigner;
use freeq_sdk::client::{self, ClientHandle, ConnectConfig};
use freeq_sdk::crypto::PrivateKey;
use freeq_sdk::event::Event;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

const OWNER: &str = "chadfowler.com";

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "irc.freeq.at:6697")]
    server: String,
    #[arg(long, default_value = "factory")]
    nick: String,
    #[arg(long, default_value = "#chad-dev")]
    channel: String,
    #[arg(long)]
    tls: bool,
}

/// Waits for a PRIVMSG from OWNER in the channel. Ignores batch (history) messages.
/// Returns the message text, or None on timeout.
async fn wait_for_owner(
    events: &mut mpsc::Receiver<Event>,
    channel: &str,
    dur: Duration,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + dur;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match timeout(remaining, events.recv()).await {
            Ok(Some(Event::Message { from, target, text, tags })) => {
                // Skip batch (history) messages
                if tags.contains_key("batch") {
                    continue;
                }
                if target.eq_ignore_ascii_case(channel)
                    && from.eq_ignore_ascii_case(OWNER)
                {
                    return Some(text);
                }
            }
            Ok(Some(_)) => continue,
            _ => return None,
        }
    }
}

/// Waits for OWNER to say "next" (or similar).
async fn wait_for_continue(
    handle: &ClientHandle,
    events: &mut mpsc::Receiver<Event>,
    channel: &str,
) -> bool {
    let _ = handle.privmsg(channel, "").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = handle.privmsg(channel, "👉 Say 'next' when you're ready to continue.").await;

    loop {
        match wait_for_owner(events, channel, Duration::from_secs(300)).await {
            None => return false,
            Some(text) => {
                let lower = text.trim().to_lowercase();
                let stripped = lower
                    .strip_prefix("factory:")
                    .or_else(|| lower.strip_prefix("factory,"))
                    .or_else(|| lower.strip_prefix("@factory"))
                    .map(|s| s.trim())
                    .unwrap_or(&lower);
                match stripped {
                    "next" | "n" | "go" | "continue" | "ok" | "yes" | "y" | "ready" => {
                        return true;
                    }
                    _ => continue,
                }
            }
        }
    }
}

/// Wait for a governance or approval signal in TagMsg events.
async fn wait_for_signal(
    events: &mut mpsc::Receiver<Event>,
    signal: &str,
    dur: Duration,
) -> Option<std::collections::HashMap<String, String>> {
    let deadline = tokio::time::Instant::now() + dur;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match timeout(remaining, events.recv()).await {
            Ok(Some(Event::TagMsg { tags, .. })) => {
                // Check for the signal in tag values
                for (k, v) in &tags {
                    if (k.contains("governance") || k.contains("event")) && v.contains(signal) {
                        return Some(tags);
                    }
                }
            }
            Ok(Some(_)) => continue,
            _ => return None,
        }
    }
}

/// Wait for either a signal OR an owner message saying "next".
/// Returns ("signal", tags) or ("next", empty) or ("timeout", empty).
async fn wait_for_signal_or_next(
    handle: &ClientHandle,
    events: &mut mpsc::Receiver<Event>,
    channel: &str,
    signal: &str,
    dur: Duration,
) -> (String, std::collections::HashMap<String, String>) {
    let deadline = tokio::time::Instant::now() + dur;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return ("timeout".into(), Default::default());
        }
        match timeout(remaining, events.recv()).await {
            Ok(Some(Event::TagMsg { tags, .. })) => {
                for (k, v) in &tags {
                    if (k.contains("governance") || k.contains("event")) && v.contains(signal) {
                        return ("signal".into(), tags);
                    }
                }
            }
            Ok(Some(Event::Message { from, target, text, tags })) => {
                if tags.contains_key("batch") {
                    continue;
                }
                if target.eq_ignore_ascii_case(channel)
                    && from.eq_ignore_ascii_case(OWNER)
                {
                    let lower = text.trim().to_lowercase();
                    let stripped = lower
                        .strip_prefix("factory:")
                        .or_else(|| lower.strip_prefix("factory,"))
                        .or_else(|| lower.strip_prefix("@factory"))
                        .map(|s| s.trim())
                        .unwrap_or(&lower);
                    if matches!(stripped, "next" | "n" | "go" | "continue" | "ok") {
                        return ("next".into(), Default::default());
                    }
                }
            }
            Ok(Some(_)) => continue,
            _ => return ("timeout".into(), Default::default()),
        }
    }
}

async fn says(handle: &ClientHandle, channel: &str, lines: &[&str]) {
    for line in lines {
        let _ = handle.privmsg(channel, line).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let args = Args::parse();
    let ch = &args.channel;

    println!("Connecting to {} as {}...", args.server, args.nick);

    // Load or generate a persistent ed25519 key for did:key auth
    let key_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".freeq/bots/factory");
    std::fs::create_dir_all(&key_dir)?;
    let key_path = key_dir.join("key.ed25519");
    let private_key = if key_path.exists() {
        let seed = std::fs::read(&key_path)?;
        PrivateKey::ed25519_from_bytes(&seed)?
    } else {
        let key = PrivateKey::generate_ed25519();
        // Save the raw 32-byte seed
        let multibase = key.public_key_multibase();
        println!("Generated new key: did:key:{multibase}");
        // We need to save the key bytes — extract from the signing key
        let bytes = key.secret_bytes();
        std::fs::write(&key_path, &bytes)?;
        key
    };
    let did = format!("did:key:{}", private_key.public_key_multibase());
    println!("DID: {did}");

    let signer = KeySigner::new(did.clone(), private_key);

    let config = ConnectConfig {
        server_addr: args.server.clone(),
        nick: args.nick.clone(),
        user: args.nick.clone(),
        realname: "Phase 2 Factory Agent".to_string(),
        tls: args.tls,
        tls_insecure: false,
        web_token: None,
    };
    let conn = client::establish_connection(&config).await?;
    let (handle, mut events) = client::connect_with_stream(conn, config, Some(std::sync::Arc::new(signer)));

    // Wait for registration
    loop {
        match events.recv().await {
            Some(Event::Registered { nick }) => {
                println!("✓ Registered as {nick}");
                break;
            }
            Some(Event::Disconnected { reason }) => {
                eprintln!("✗ Disconnected: {reason}");
                return Ok(());
            }
            _ => continue,
        }
    }

    // Register as agent, set provenance, join channel
    handle.register_agent("agent").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    handle.raw("HEARTBEAT 60").await?;
    handle.raw("PRESENCE :state=idle;status=Waiting for instructions").await?;
    handle.join(ch).await?;

    // Drain history — wait for NamesEnd then skip batch messages for a few seconds
    println!("Waiting for history to finish...");
    let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match timeout(
            drain_deadline.saturating_duration_since(tokio::time::Instant::now()),
            events.recv(),
        )
        .await
        {
            Ok(Some(Event::NamesEnd { .. })) => {
                // Keep draining batch messages for a bit
                tokio::time::sleep(Duration::from_secs(3)).await;
                // Drain remaining
                while let Ok(Some(_)) = timeout(Duration::from_millis(100), events.recv()).await {}
                break;
            }
            Ok(Some(_)) => continue,
            _ => break,
        }
    }
    println!("Ready. Starting demo.");

    // ─── Intro ──────────────────────────────────────
    says(&handle, ch, &[
        "👋 Hey! I'm factory — a demo agent for Phase 2: Governable Agents.",
        "Phase 1 made agents visible. Phase 2 makes them controllable.",
        "I'll walk through each governance feature, one at a time.",
        "There are 5 features to demo.",
    ]).await;

    if !wait_for_continue(&handle, &mut events, ch).await {
        return Ok(());
    }

    // ─── Step 1: Governance Signals ─────────────────
    says(&handle, ch, &[
        "",
        "━━━ Step 1 of 5: Governance Signals (Pause / Resume / Revoke) ━━━",
        "",
        "Channel ops can control agents in real time using three commands:",
        "   AGENT PAUSE <nick> [reason]",
        "   AGENT RESUME <nick>",
        "   AGENT REVOKE <nick> [reason]",
        "",
        "These are IRC commands you type in any client. Try it now:",
        "   /quote AGENT PAUSE factory too noisy",
        "",
        "I'll react immediately — watch my presence state change.",
        "",
        "👉 Try pausing me! Or say 'next' to skip and I'll simulate it.",
    ]).await;

    let (result, _) =
        wait_for_signal_or_next(&handle, &mut events, ch, "pause", Duration::from_secs(120))
            .await;

    if result == "signal" {
        // Actually paused by governance
        handle.raw("PRESENCE :state=paused;status=Paused by channel op").await?;
        says(&handle, ch, &[
            "⏸️ I've been paused! My presence state is now 'paused'.",
            "I won't do any work until I'm resumed.",
            "",
            "Now resume me: /quote AGENT RESUME factory",
        ]).await;

        // Wait for resume
        wait_for_signal(&mut events, "resume", Duration::from_secs(120)).await;
        handle.raw("PRESENCE :state=active;status=Resumed").await?;
        says(&handle, ch, &["▶️ Resumed! Back to work."]).await;
    } else {
        // Simulated
        says(&handle, ch, &[
            "",
            "(Simulating pause/resume since you said 'next')",
        ]).await;
        handle.raw("PRESENCE :state=paused;status=Paused by governance demo").await?;
        says(&handle, ch, &["⏸️ [simulated] I'm now paused. Presence state = paused."]).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        handle.raw("PRESENCE :state=active;status=Resumed from governance demo").await?;
        says(&handle, ch, &["▶️ [simulated] Resumed. Presence state = active."]).await;
    }

    says(&handle, ch, &[
        "",
        "Key points about governance signals:",
        "   • They're delivered as IRCv3 TAGMSG with structured tags",
        "   • The agent receives them instantly and reacts",
        "   • Everyone in the channel sees a human-readable NOTICE",
        "   • Legacy clients (irssi, weechat) see it as plain text",
        "   • REVOKE is permanent — the agent disconnects gracefully",
    ]).await;

    if !wait_for_continue(&handle, &mut events, ch).await {
        return Ok(());
    }

    // ─── Step 2: Approval Flows ────────────────────
    says(&handle, ch, &[
        "",
        "━━━ Step 2 of 5: Approval Flows ━━━",
        "",
        "Some actions are too risky for an agent to do autonomously.",
        "The approval flow works like this:",
        "",
        "  1. Agent requests approval: APPROVAL_REQUEST #channel :deploy",
        "  2. Server notifies channel ops with a NOTICE",
        "  3. Op approves: AGENT APPROVE factory deploy",
        "  4. Agent receives approval TAGMSG and proceeds",
        "",
        "Let me demonstrate. I'll request approval to 'deploy'...",
    ]).await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    handle.raw("PRESENCE :state=blocked_on_permission;status=Awaiting deploy approval").await?;
    handle
        .raw(&format!("APPROVAL_REQUEST {ch} :deploy;resource=landing-page-v2"))
        .await?;

    says(&handle, ch, &[
        "",
        "🔔 I just sent: APPROVAL_REQUEST — requesting permission to deploy.",
        "My presence is now 'blocked_on_permission'.",
        "",
        "You should see a server NOTICE asking you to approve.",
        "To approve: /quote AGENT APPROVE factory deploy",
        "To deny: /quote AGENT DENY factory deploy",
        "Or say 'next' to skip.",
    ]).await;

    let (result, _) = wait_for_signal_or_next(
        &handle,
        &mut events,
        ch,
        "approval_granted",
        Duration::from_secs(120),
    )
    .await;

    // Also check for denial
    if result == "signal" {
        handle.raw("PRESENCE :state=executing;status=Deploying landing-page-v2").await?;
        says(&handle, ch, &["✅ Approval granted! Deploying..."]).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        says(&handle, ch, &["🚀 Deploy complete: landing-page-v2 is live!"]).await;
        handle.raw("PRESENCE :state=active;status=Deploy complete").await?;
    } else {
        says(&handle, ch, &["(Simulating approval since you said 'next')"]).await;
        handle.raw("PRESENCE :state=executing;status=Deploying landing-page-v2").await?;
        says(&handle, ch, &["✅ [simulated] Approval granted. Deploying..."]).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        says(&handle, ch, &["🚀 [simulated] Deploy complete!"]).await;
        handle.raw("PRESENCE :state=active;status=Deploy complete").await?;
    }

    if !wait_for_continue(&handle, &mut events, ch).await {
        return Ok(());
    }

    // ─── Step 3: Spawning Child Agents ─────────────
    says(&handle, ch, &[
        "",
        "━━━ Step 3 of 5: Spawning Child Agents ━━━",
        "",
        "A parent agent can spawn short-lived child agents for subtasks.",
        "Children inherit capabilities from the parent and have a TTL.",
        "",
        "I'll spawn a child called 'factory-worker' with a 5-minute TTL:",
    ]).await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    handle
        .raw(&format!(
            "AGENT SPAWN {ch} :nick=factory-worker;capabilities=post_message;ttl=300;task=build-css"
        ))
        .await?;
    tokio::time::sleep(Duration::from_secs(1)).await;

    says(&handle, ch, &[
        "✅ Spawned factory-worker! It should have joined the channel.",
        "It has:",
        "   • nick: factory-worker",
        "   • capabilities: post_message",
        "   • TTL: 300 seconds (auto-despawn)",
        "   • task: build-css",
        "   • parent: factory (me)",
        "",
        "I can send messages as the child:",
    ]).await;

    handle.raw(&format!("AGENT MSG factory-worker {ch} :🔨 Working on CSS compilation...")).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    handle.raw(&format!("AGENT MSG factory-worker {ch} :✅ CSS compiled successfully!")).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;

    says(&handle, ch, &[
        "",
        "Now I'll despawn it:",
    ]).await;
    handle.raw("AGENT DESPAWN factory-worker").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    says(&handle, ch, &[
        "✅ factory-worker despawned.",
        "",
        "In the web client, spawned children show the parent in their identity card.",
        "Legacy clients just see them as normal nicks that join and part.",
    ]).await;

    if !wait_for_continue(&handle, &mut events, ch).await {
        return Ok(());
    }

    // ─── Step 4: Heartbeat Enforcement ─────────────
    says(&handle, ch, &[
        "",
        "━━━ Step 4 of 5: Heartbeat Enforcement ━━━",
        "",
        "Phase 1 introduced heartbeat. Phase 2 enforces it.",
        "If an agent stops heartbeating, the server automatically:",
        "",
        "   1× TTL (60s):  transitions to 'degraded' 🟡",
        "   2× TTL (120s): transitions to 'offline' ⚫",
        "   5× TTL (300s): force disconnects the agent",
        "",
        "This prevents zombie agents from occupying channels forever.",
        "The server doesn't trust agents to self-report — it watches the clock.",
    ]).await;

    handle.raw("HEARTBEAT 60").await?;
    says(&handle, ch, &[
        "",
        "✅ HEARTBEAT 60 sent just now.",
        "If I crash, the server detects it and cleans up automatically.",
        "No orphaned bots. No stale member lists. No manual cleanup.",
    ]).await;

    if !wait_for_continue(&handle, &mut events, ch).await {
        return Ok(());
    }

    // ─── Step 5: Full Governance Loop ──────────────
    says(&handle, ch, &[
        "",
        "━━━ Step 5 of 5: The Full Governance Loop ━━━",
        "",
        "Let me show the full loop as a realistic scenario:",
        "",
        "Scenario: You ask me to build and deploy a landing page.",
    ]).await;

    // Phase: Working
    handle.raw("PRESENCE :state=active;status=Accepted task: build landing page").await?;
    says(&handle, ch, &[
        "👍 Building a landing page. Here's my plan:",
        "   1. Generate HTML/CSS",
        "   2. Request deploy approval",
        "   3. Deploy (if approved)",
    ]).await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    handle.raw("PRESENCE :state=executing;status=Generating HTML and CSS").await?;
    says(&handle, ch, &["🔨 Generating HTML..."]).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    says(&handle, ch, &["🔨 Generating CSS..."]).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    says(&handle, ch, &["✅ Build complete. Ready to deploy."]).await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Phase: Request approval
    handle
        .raw("PRESENCE :state=blocked_on_permission;status=Awaiting deploy approval for landing page")
        .await?;
    handle
        .raw(&format!("APPROVAL_REQUEST {ch} :deploy;resource=landing-page"))
        .await?;

    says(&handle, ch, &[
        "",
        "🔔 I need approval to deploy. My presence is now 'blocked_on_permission'.",
        "",
        "Approve: /quote AGENT APPROVE factory deploy",
        "Deny: /quote AGENT DENY factory deploy not yet",
        "Or say 'next' to simulate approval.",
    ]).await;

    let (result, _) = wait_for_signal_or_next(
        &handle,
        &mut events,
        ch,
        "approval_granted",
        Duration::from_secs(180),
    )
    .await;

    if result == "signal" {
        handle.raw("PRESENCE :state=executing;status=Deploying landing page").await?;
        says(&handle, ch, &["✅ Approval granted! Deploying..."]).await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        says(&handle, ch, &["🚀 Deployed! https://landing-page.example.com is live."]).await;
        handle.raw("PRESENCE :state=idle;status=Task complete — landing page deployed").await?;
    } else {
        says(&handle, ch, &["(Simulating approval)"]).await;
        handle.raw("PRESENCE :state=executing;status=Deploying landing page").await?;
        says(&handle, ch, &["✅ [simulated] Deploying..."]).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        says(&handle, ch, &["🚀 [simulated] Deployed!"]).await;
        handle.raw("PRESENCE :state=idle;status=Task complete").await?;
    }

    // ─── Summary ───────────────────────────────────
    says(&handle, ch, &[
        "",
        "━━━ Phase 2: Governable Agents — Summary ━━━",
        "",
        "What we demonstrated:",
        "   1. ⏸️ Pause/Resume/Revoke — real-time agent governance",
        "   2. 🔔 Approval flows — agents request permission for risky actions",
        "   3. 👶 Child agents — parent spawns workers with TTL",
        "   4. 💓 Heartbeat enforcement — server detects dead agents",
        "   5. 🔄 Full governance loop — task → build → approve → deploy",
        "",
        "Everything visible in plain text for legacy IRC clients.",
        "Rich clients get structured TAGMSG tags for UI integration.",
        "",
        "Phase 1 answered: 'Who is this agent?'",
        "Phase 2 answers: 'What can it do, and who controls it?'",
        "",
        "👋 factory signing off. Demo complete!",
    ]).await;

    handle.raw("PRESENCE :state=offline;status=Demo complete").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    handle.quit(Some("Phase 2 demo complete")).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("Done.");
    Ok(())
}

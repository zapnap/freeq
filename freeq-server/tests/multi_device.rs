//! Multi-device session tests.
//!
//! Tests ghost session recovery, multi-device channel sync, and
//! the attach_same_did flow with two authenticated SDK clients.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use freeq_sdk::auth::{ChallengeSigner, KeySigner};
use freeq_sdk::client::{self, ConnectConfig};
use freeq_sdk::crypto::PrivateKey;
use freeq_sdk::did::{self, DidResolver};
use freeq_sdk::event::Event;
use tokio::sync::mpsc;
use tokio::time::timeout;

const DID: &str = "did:plc:multidev";
const TIMEOUT_MS: u64 = 5000;

async fn start(key: &PrivateKey) -> (std::net::SocketAddr, tokio::task::JoinHandle<anyhow::Result<()>>) {
    let doc = did::make_test_did_document(DID, &key.public_key_multibase());
    let mut docs = HashMap::new();
    docs.insert(DID.to_string(), doc);
    let resolver = DidResolver::static_map(docs);
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let db = tmp.path().to_str().unwrap().to_string();
    std::mem::forget(tmp);
    let config = freeq_server::config::ServerConfig {
        listen_addr: "127.0.0.1:0".to_string(),
        server_name: "test-multidev".to_string(),
        challenge_timeout_secs: 60,
        db_path: Some(db),
        ..Default::default()
    };
    freeq_server::server::Server::with_resolver(config, resolver)
        .start().await.unwrap()
}

async fn connect_as_did(addr: std::net::SocketAddr, nick: &str, key: PrivateKey) -> (client::ClientHandle, mpsc::Receiver<Event>) {
    let signer: Arc<dyn ChallengeSigner> = Arc::new(KeySigner::new(DID.to_string(), key));
    let config = ConnectConfig {
        server_addr: addr.to_string(),
        nick: nick.to_string(),
        user: nick.to_string(),
        realname: "multi-device test".to_string(),
        ..Default::default()
    };
    client::connect(config, Some(signer))
}

async fn wait_event(rx: &mut mpsc::Receiver<Event>, pred: impl Fn(&Event) -> bool, desc: &str) -> Event {
    timeout(Duration::from_millis(TIMEOUT_MS), async {
        loop {
            match rx.recv().await {
                Some(e) if pred(&e) => return e,
                Some(_) => continue,
                None => panic!("Channel closed: {desc}"),
            }
        }
    }).await.unwrap_or_else(|_| panic!("Timeout: {desc}"))
}

async fn wait_registered(rx: &mut mpsc::Receiver<Event>) -> String {
    match wait_event(rx, |e| matches!(e, Event::Registered { .. }), "Registered").await {
        Event::Registered { nick } => nick,
        _ => unreachable!(),
    }
}

async fn wait_auth(rx: &mut mpsc::Receiver<Event>) {
    wait_event(rx, |e| matches!(e, Event::Authenticated { .. }), "Authenticated").await;
}

// ═══════════════════════════════════════════════════════════════
// GHOST SESSION: disconnect and reconnect within grace period
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn ghost_reconnect_within_grace_period() {
    let key = PrivateKey::generate_ed25519();
    let key2 = PrivateKey::ed25519_from_bytes(&key.secret_bytes()).unwrap();
    let (addr, _h) = start(&key).await;

    // Device 1: connect, auth, join channel
    let (h1, mut rx1) = connect_as_did(addr, "ghostuser", key).await;
    wait_auth(&mut rx1).await;
    wait_registered(&mut rx1).await;
    h1.join("#ghost").await.unwrap();
    wait_event(&mut rx1, |e| matches!(e, Event::Joined { channel, .. } if channel == "#ghost"), "Joined").await;
    h1.privmsg("#ghost", "before disconnect").await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Disconnect (triggers ghost mode)
    h1.quit(None).await.ok();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Device 2: reconnect with same DID within 30s grace period
    let (h2, mut rx2) = connect_as_did(addr, "ghostuser", key2).await;
    wait_auth(&mut rx2).await;
    let nick = wait_registered(&mut rx2).await;
    assert_eq!(nick.to_lowercase(), "ghostuser", "Should reclaim same nick");

    // Should be able to send to #ghost (membership restored)
    h2.privmsg("#ghost", "after reconnect").await.unwrap();
    // If we get an echo or no error, membership was restored
    tokio::time::sleep(Duration::from_millis(500)).await;

    h2.quit(None).await.ok();
}

// ═══════════════════════════════════════════════════════════════
// MULTI-DEVICE: two clients, same DID, simultaneous
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn two_devices_same_did_both_authenticated() {
    let key = PrivateKey::generate_ed25519();
    let key1 = PrivateKey::ed25519_from_bytes(&key.secret_bytes()).unwrap();
    let key2 = PrivateKey::ed25519_from_bytes(&key.secret_bytes()).unwrap();
    let (addr, _h) = start(&key).await;

    // Device 1
    let (h1, mut rx1) = connect_as_did(addr, "dev1", key1).await;
    wait_auth(&mut rx1).await;
    wait_registered(&mut rx1).await;
    h1.join("#multi").await.unwrap();
    wait_event(&mut rx1, |e| matches!(e, Event::Joined { channel, .. } if channel == "#multi"), "D1 Joined").await;

    // Device 2 (same DID)
    let (h2, mut rx2) = connect_as_did(addr, "dev1", key2).await;
    wait_auth(&mut rx2).await;
    wait_registered(&mut rx2).await;

    // Device 2 should be auto-attached to #multi (from device 1's membership)
    // It gets synthetic JOINs during attach_same_did
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Device 1 sends message — device 2 should receive it
    h1.privmsg("#multi", "from device 1").await.unwrap();
    let msg = wait_event(&mut rx2, |e| matches!(e, Event::Message { text, .. } if text == "from device 1"), "D2 receives D1 msg").await;
    assert!(matches!(msg, Event::Message { text, .. } if text == "from device 1"));

    // Device 2 sends message — device 1 should receive it
    h2.privmsg("#multi", "from device 2").await.unwrap();
    let msg = wait_event(&mut rx1, |e| matches!(e, Event::Message { text, .. } if text == "from device 2"), "D1 receives D2 msg").await;
    assert!(matches!(msg, Event::Message { text, .. } if text == "from device 2"));

    h1.quit(None).await.ok();
    h2.quit(None).await.ok();
}

#[tokio::test]
async fn multi_device_one_disconnects_other_continues() {
    let key = PrivateKey::generate_ed25519();
    let key1 = PrivateKey::ed25519_from_bytes(&key.secret_bytes()).unwrap();
    let key2 = PrivateKey::ed25519_from_bytes(&key.secret_bytes()).unwrap();
    let (addr, _h) = start(&key).await;

    let (h1, mut rx1) = connect_as_did(addr, "persist", key1).await;
    wait_auth(&mut rx1).await;
    wait_registered(&mut rx1).await;
    h1.join("#persist").await.unwrap();
    wait_event(&mut rx1, |e| matches!(e, Event::Joined { channel, .. } if channel == "#persist"), "Joined").await;

    let (h2, mut rx2) = connect_as_did(addr, "persist", key2).await;
    wait_auth(&mut rx2).await;
    wait_registered(&mut rx2).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Device 1 disconnects
    h1.quit(None).await.ok();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Device 2 should still work — send message
    h2.privmsg("#persist", "still here").await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    h2.quit(None).await.ok();
}

// ═══════════════════════════════════════════════════════════════
// GUEST + AUTH: guest cannot claim authenticated nick
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn guest_cannot_claim_did_owned_nick() {
    let key = PrivateKey::generate_ed25519();
    let key_copy = PrivateKey::ed25519_from_bytes(&key.secret_bytes()).unwrap();
    let (addr, _h) = start(&key).await;

    // Authenticated user claims "owner" nick
    let (h1, mut rx1) = connect_as_did(addr, "owner", key_copy).await;
    wait_auth(&mut rx1).await;
    wait_registered(&mut rx1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Guest tries to use same nick
    let config = ConnectConfig {
        server_addr: addr.to_string(),
        nick: "owner".to_string(),
        user: "guest".to_string(),
        realname: "guest".to_string(),
        ..Default::default()
    };
    let (_h2, mut rx2) = client::connect(config, None);
    let nick = wait_registered(&mut rx2).await;
    // Guest should get a different nick (renamed to Guest*)
    assert_ne!(nick.to_lowercase(), "owner", "Guest should not get DID-owned nick, got: {nick}");

    h1.quit(None).await.ok();
}

//! Tests for the narrow-scope + step-up OAuth changes (Phase 1 + Phase 2).
//!
//! Phase 1 invariants:
//!   - The published client-metadata.json does NOT advertise the legacy
//!     `transition:generic` scope. It declares the union of narrow scopes
//!     this client may request across all flows.
//!
//! Phase 2 invariants:
//!   - `/auth/step-up` rejects requests with no active Login session
//!     for the named DID (so it can't be used as a primary login).
//!   - `/auth/step-up` rejects unknown purposes and the `login` purpose.
//!   - `scope_satisfies_purpose` correctly accepts both granular and
//!     legacy `transition:generic` grants for upload, and only the
//!     specific `repo:app.bsky.feed.post` grant for cross-post.

use freeq_sdk::did::DidResolver;
use std::collections::HashMap;
use std::net::SocketAddr;

use freeq_server::server::{OauthPurpose, scope_satisfies_purpose};

async fn start_server() -> (
    SocketAddr,
    SocketAddr,
    tokio::task::JoinHandle<anyhow::Result<()>>,
) {
    let resolver = DidResolver::static_map(HashMap::new());
    let config = freeq_server::config::ServerConfig {
        listen_addr: "127.0.0.1:0".to_string(),
        server_name: "test-oauth-scope".to_string(),
        challenge_timeout_secs: 60,
        ..Default::default()
    };
    let server = freeq_server::server::Server::with_resolver(config, resolver);
    server.start_with_web().await.unwrap()
}

fn url(http: SocketAddr, path: &str) -> String {
    format!("http://{http}{path}")
}

// ─── Phase 1 ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn client_metadata_does_not_advertise_transition_generic() {
    let (_irc, http, _h) = start_server().await;
    let body: serde_json::Value = reqwest::Client::new()
        .get(url(http, "/client-metadata.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let scope = body["scope"].as_str().expect("scope field present");
    assert!(
        !scope.contains("transition:generic"),
        "client-metadata.json must not advertise the legacy `transition:generic` scope; \
         got: {scope}"
    );
    assert!(
        scope.contains("atproto"),
        "scope must still include `atproto` for identity proof; got: {scope}"
    );
    // The metadata is the *union* of all flows the client may request, so
    // we expect to see at least the granular blob upload scope listed.
    assert!(
        scope.contains("blob:image/"),
        "scope union should advertise blob:image/* so step-up flows can request it; got: {scope}"
    );
}

// ─── Phase 2 ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn step_up_rejects_when_no_login_session_exists() {
    // The endpoint is meant for *upgrading* an existing login. Without a
    // primary login it must refuse — otherwise it would be a back-door
    // sign-in that skips the consent screen for chat permissions.
    let (_irc, http, _h) = start_server().await;
    let resp = reqwest::Client::new()
        .get(url(
            http,
            "/auth/step-up?purpose=blob_upload&did=did:plc:notlogged",
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn step_up_rejects_unknown_purpose() {
    let (_irc, http, _h) = start_server().await;
    let resp = reqwest::Client::new()
        .get(url(
            http,
            "/auth/step-up?purpose=become_admin&did=did:plc:abc",
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn step_up_refuses_login_purpose() {
    // `login` is a valid OauthPurpose internally but isn't a step-up —
    // the primary login endpoint already covers it. Refuse to avoid
    // confusing dual paths.
    let (_irc, http, _h) = start_server().await;
    let resp = reqwest::Client::new()
        .get(url(http, "/auth/step-up?purpose=login&did=did:plc:abc"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
}

// ─── Scope predicate ─────────────────────────────────────────────────────

#[test]
fn scope_satisfies_login_with_granular_grant() {
    assert!(scope_satisfies_purpose("atproto", OauthPurpose::Login));
    assert!(scope_satisfies_purpose(
        "atproto blob:image/*",
        OauthPurpose::Login
    ));
}

#[test]
fn scope_satisfies_blob_upload_with_granular_grant() {
    assert!(scope_satisfies_purpose(
        "atproto blob:image/*",
        OauthPurpose::BlobUpload
    ));
    assert!(scope_satisfies_purpose(
        "atproto blob:*/*",
        OauthPurpose::BlobUpload
    ));
    // Identity-only grant must NOT be enough for upload.
    assert!(!scope_satisfies_purpose("atproto", OauthPurpose::BlobUpload));
}

#[test]
fn scope_satisfies_legacy_transition_generic_for_anything() {
    // Older PDSes downgrade granular requests to transition:generic.
    // Treat that as satisfying any purpose for backward compatibility.
    assert!(scope_satisfies_purpose(
        "atproto transition:generic",
        OauthPurpose::BlobUpload
    ));
    assert!(scope_satisfies_purpose(
        "atproto transition:generic",
        OauthPurpose::BlueskyPost
    ));
}

#[test]
fn scope_does_not_satisfy_bluesky_post_with_blob_only() {
    assert!(!scope_satisfies_purpose(
        "atproto blob:image/*",
        OauthPurpose::BlueskyPost
    ));
    assert!(scope_satisfies_purpose(
        "atproto repo:app.bsky.feed.post",
        OauthPurpose::BlueskyPost
    ));
}

#[test]
fn purpose_round_trips_through_string() {
    for p in [
        OauthPurpose::Login,
        OauthPurpose::BlobUpload,
        OauthPurpose::BlueskyPost,
    ] {
        let s = p.as_str();
        assert_eq!(OauthPurpose::from_str(s), Some(p));
    }
    assert_eq!(OauthPurpose::from_str("nonsense"), None);
}

#[test]
fn requested_scopes_are_narrow_not_transition_generic() {
    for p in [
        OauthPurpose::Login,
        OauthPurpose::BlobUpload,
        OauthPurpose::BlueskyPost,
    ] {
        let s = p.requested_scope();
        assert!(
            !s.contains("transition:generic"),
            "purpose {:?} requests legacy scope: {s}",
            p
        );
        assert!(s.contains("atproto"), "purpose {:?} missing atproto: {s}", p);
    }
}

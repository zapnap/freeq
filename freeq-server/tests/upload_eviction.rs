//! Tests for `handle_pds_upload_failure` — eviction of stale `web_sessions`
//! entries when the PDS rejects a blob upload, and the structured 401 body
//! clients use to drive `ReauthFlow`.

use std::collections::HashMap;

use axum::http::StatusCode;
use freeq_server::server::{OauthPurpose, WebSession};
use freeq_server::web::handle_pds_upload_failure;
use parking_lot::Mutex;

const TEST_DID: &str = "did:plc:test_evict";

fn fake_session() -> WebSession {
    WebSession {
        did: TEST_DID.to_string(),
        handle: "alice.bsky.social".to_string(),
        pds_url: "https://example.invalid".to_string(),
        access_token: "dead-token".to_string(),
        dpop_key_b64: "AAAA".to_string(),
        dpop_nonce: None,
        created_at: std::time::Instant::now(),
        granted_scope: "atproto blob:image/*".to_string(),
    }
}

fn make_sessions(
    purposes: &[OauthPurpose],
) -> Mutex<HashMap<(String, OauthPurpose), WebSession>> {
    let mut map = HashMap::new();
    for p in purposes {
        map.insert((TEST_DID.to_string(), *p), fake_session());
    }
    Mutex::new(map)
}

fn parse_json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).expect("body should be JSON")
}

#[test]
fn non_auth_error_returns_bad_gateway_and_does_not_evict() {
    let sessions = make_sessions(&[OauthPurpose::Login, OauthPurpose::BlobUpload]);
    let (status, body) =
        handle_pds_upload_failure(&sessions, TEST_DID, "503 service unavailable");
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(body.contains("PDS upload failed"));
    let map = sessions.lock();
    assert!(map.contains_key(&(TEST_DID.to_string(), OauthPurpose::Login)));
    assert!(map.contains_key(&(TEST_DID.to_string(), OauthPurpose::BlobUpload)));
}

#[test]
fn expired_message_evicts_both_sessions_and_returns_structured_401() {
    let sessions = make_sessions(&[OauthPurpose::Login, OauthPurpose::BlobUpload]);
    let msg = "Blob upload to PDS failed — your session may have expired. Please sign in again.";
    let (status, body) = handle_pds_upload_failure(&sessions, TEST_DID, msg);
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let json = parse_json(&body);
    assert_eq!(json["error"], "session_expired");
    assert_eq!(json["action"], "reauth_required");
    assert!(json["message"].as_str().unwrap().contains("expired"));
    assert_eq!(json["detail"], msg);
    let map = sessions.lock();
    assert!(!map.contains_key(&(TEST_DID.to_string(), OauthPurpose::Login)));
    assert!(!map.contains_key(&(TEST_DID.to_string(), OauthPurpose::BlobUpload)));
}

#[test]
fn explicit_401_in_message_evicts() {
    let sessions = make_sessions(&[OauthPurpose::Login]);
    let (status, _body) =
        handle_pds_upload_failure(&sessions, TEST_DID, "PDS returned 401: token bad");
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(sessions.lock().is_empty());
}

#[test]
fn evicting_with_no_existing_sessions_is_safe() {
    let sessions = make_sessions(&[]);
    let (status, body) =
        handle_pds_upload_failure(&sessions, TEST_DID, "Authentication expired");
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let json = parse_json(&body);
    assert_eq!(json["error"], "session_expired");
}

#[test]
fn eviction_only_targets_the_specified_did() {
    let other_did = "did:plc:bystander";
    let mut map = HashMap::new();
    map.insert((TEST_DID.to_string(), OauthPurpose::Login), fake_session());
    map.insert(
        (other_did.to_string(), OauthPurpose::Login),
        WebSession {
            did: other_did.to_string(),
            ..fake_session()
        },
    );
    let sessions = Mutex::new(map);
    let (_s, _b) = handle_pds_upload_failure(&sessions, TEST_DID, "401");
    let map = sessions.lock();
    assert!(!map.contains_key(&(TEST_DID.to_string(), OauthPurpose::Login)));
    assert!(map.contains_key(&(other_did.to_string(), OauthPurpose::Login)));
}

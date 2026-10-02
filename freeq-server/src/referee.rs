//! Whether a ruling on a task is the word of the server its opener named.
//!
//! An opener may carry `act-home`: the `did:web:` name of the server that
//! referees the task, signed by the person who posted it. A ruling on such a
//! task — a receipt, an expiry, a closed review window — counts when it is
//! signed by that name with a key the named server's own host lists, however
//! it reached this server. Nothing a peer says decides it: the key comes from
//! the referee's `/.well-known/did.json` (`#freeq`) or from its own key route,
//! `/api/v1/signing-keys/<did>/<kid>`, on the same host, and never from a key
//! another server handed over.
//!
//! Three answers, kept apart:
//!
//! - **listed**: the referee's host holds the key. Kept for ever: a key id is
//!   a hash of the key, so the answer cannot go stale.
//! - **not listed**: the host answered and does not hold it. The ruling does
//!   not count. Used to judge what was waiting on it, then dropped, so a key
//!   id is always asked again before it is refused.
//! - **cannot answer**: the document could not be read or names no `#freeq`
//!   key, or the key route failed. The ruling is judged as before this
//!   existed, by the link it came on. Remembered for `--peer-key-retry-secs`,
//!   so a server whose name is not a real host is not asked once per ruling.
//!
//! A ruling waiting for an answer is parked in the defer queue under the
//! referee's DID and the key id, and the answer releases it.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::server::SharedState;

/// How long one request to a referee's host may take.
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

type Pair = (String, String);
/// A key's bytes, and when it stopped counting (unix seconds), if it has.
type ListedKey = ([u8; 32], Option<i64>);

/// Keys a referee's own host has listed, by `(did, kid)`, each with when it
/// stopped counting (unix seconds), if it has.
static LISTED: LazyLock<Mutex<HashMap<Pair, ListedKey>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// A host that answered "not listed", only while what waited on it is judged.
static NOT_LISTED: LazyLock<Mutex<HashSet<Pair>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
/// When a referee last could not answer for a key. Bounded: entries older
/// than the retry window are dropped, and past [`MAX_CANNOT_ANSWER`] the
/// oldest goes, so a peer naming new keys cannot grow it without limit.
static CANNOT_ANSWER: LazyLock<Mutex<HashMap<Pair, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// The most "cannot answer" entries remembered at once.
const MAX_CANNOT_ANSWER: usize = 1024;
/// Questions on their way to a referee's host.
static ASKING: LazyLock<Mutex<HashSet<Pair>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// What the referee's own host says about one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Listing {
    /// It lists this key, which stopped counting at the second given, if it
    /// has: retired, or expired.
    Listed(ed25519_dalek::VerifyingKey, Option<i64>),
    /// It answered and does not list it.
    NotListed,
    /// It could not be asked: today's rule applies.
    CannotAnswer,
    /// Not asked yet.
    Unknown,
}

/// Whether `verb` is one the home signs about a task: its receipt, an
/// expiry, or the review window closing.
pub(crate) fn is_ruling(verb: &str) -> bool {
    freeq_sdk::act_transitions::is_confirmation(verb)
        || verb == "expire"
        || verb == freeq_sdk::act_transitions::REVIEW_TIMEOUT_VERB
}

/// What is known now about `kid` under the referee `did`, without asking.
///
/// This server is its own referee's host: its own name's keys are the rows
/// it holds under that name, which nothing but this server writes.
pub(crate) fn answer(state: &Arc<SharedState>, did: &str, kid: &str) -> Listing {
    if did == crate::server::server_did(&state.server_name) {
        return match state
            .with_db(|db| db.get_signing_key_row(did, kid))
            .flatten()
            .and_then(|row| {
                let key = ed25519_dalek::VerifyingKey::from_bytes(&row.pubkey).ok()?;
                Some((key, stopped_at(row.removed_at, row.expires_at)))
            }) {
            Some((key, stopped)) => Listing::Listed(key, stopped),
            None => Listing::NotListed,
        };
    }
    let pair = (did.to_string(), kid.to_string());
    if let Some((bytes, stopped)) = LISTED.lock().get(&pair).copied()
        && let Ok(key) = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
    {
        return Listing::Listed(key, stopped);
    }
    if NOT_LISTED.lock().contains(&pair) {
        return Listing::NotListed;
    }
    // Remembered for the retry window, and always while what waited on the
    // answer is being judged.
    let remembered = Duration::from_secs(state.config.peer_key_retry_secs);
    if CANNOT_ANSWER
        .lock()
        .get(&pair)
        .is_some_and(|at| at.elapsed() < remembered || ASKING.lock().contains(&pair))
    {
        return Listing::CannotAnswer;
    }
    Listing::Unknown
}

/// The earlier of a key's retirement and its expiry, unix seconds.
fn stopped_at(removed_at: Option<i64>, expires_at: Option<i64>) -> Option<i64> {
    removed_at.into_iter().chain(expires_at).min()
}

/// Whether a ruling under `event_id` was signed while its key still counted:
/// a signature made at or after the key stopped counting does not (plan
/// ruling 24). The event id's own time dates the signature; one that does
/// not read as a time is taken as now.
pub(crate) fn signed_while_live(stopped: Option<i64>, event_id: &str) -> bool {
    let at_ms = freeq_sdk::sigtag::msgid_timestamp_ms(event_id)
        .and_then(|ms| i64::try_from(ms).ok())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    stopped.is_none_or(|stopped| at_ms < stopped.saturating_mul(1000))
}

/// Ask the referee's own host about `kid`, off the delivery path. At most one
/// question per `(did, kid)` is out at a time; its answer releases whatever
/// was parked on that pair.
pub(crate) fn ask(state: &Arc<SharedState>, did: &str, kid: &str) {
    let pair = (did.to_string(), kid.to_string());
    if !ASKING.lock().insert(pair.clone()) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let (did, kid) = pair;
        let answer = fetch(&did, &kid).await;
        tracing::info!(did = %did, kid = %kid, answer = ?answer, "A task's referee answered for a key");
        settle(&state, &did, &kid, answer);
    });
}

/// File an answer and judge what waited on it.
fn settle(state: &Arc<SharedState>, did: &str, kid: &str, answer: Listing) {
    let pair = (did.to_string(), kid.to_string());
    match &answer {
        Listing::Listed(key, stopped) => {
            LISTED
                .lock()
                .insert(pair.clone(), (key.to_bytes(), *stopped));
        }
        Listing::NotListed => {
            NOT_LISTED.lock().insert(pair.clone());
        }
        Listing::CannotAnswer | Listing::Unknown => {
            remember_cannot_answer(state, pair.clone());
        }
    }
    crate::server::retry_deferred_task_events(state, did, kid);
    NOT_LISTED.lock().remove(&pair);
    ASKING.lock().remove(&pair);
}

/// Remember that a referee could not answer for `pair`, dropping what the
/// retry window has passed and, past the bound, the oldest entry.
fn remember_cannot_answer(state: &Arc<SharedState>, pair: Pair) {
    let window = Duration::from_secs(state.config.peer_key_retry_secs);
    let mut held = CANNOT_ANSWER.lock();
    held.retain(|_, at| at.elapsed() < window);
    while held.len() >= MAX_CANNOT_ANSWER {
        let Some(oldest) = held
            .iter()
            .min_by_key(|(_, at)| **at)
            .map(|(k, _)| k.clone())
        else {
            break;
        };
        held.remove(&oldest);
    }
    held.insert(pair, Instant::now());
}

/// The base URL of a `did:web:` name's own host, or `None` for a name that is
/// not a plain host name (a port or a path), which is never asked.
fn host_base(did: &str) -> Option<String> {
    let host = did.strip_prefix("did:web:")?;
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return None;
    }
    #[cfg(test)]
    if let Some(base) = TEST_HOSTS.lock().get(host) {
        return Some(base.clone());
    }
    Some(format!("https://{host}"))
}

/// One GET to the referee's host: the status, and the body when it is 2xx.
async fn get(url: &str) -> anyhow::Result<(u16, Option<serde_json::Value>)> {
    #[cfg(test)]
    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build()?;
    #[cfg(not(test))]
    let client = {
        use freeq_oauth::ClientProvider;
        crate::web::SsrfClients {
            timeout: FETCH_TIMEOUT,
        }
        .client_for(url)
        .await?
    };
    let response = client.get(url).send().await?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Ok((status, None));
    }
    Ok((status, Some(response.json().await?)))
}

/// Ask the referee's own host whether it holds `kid`: its document's
/// `#freeq` key first, then its own key route by key id.
async fn fetch(did: &str, kid: &str) -> Listing {
    let Some(base) = host_base(did) else {
        return Listing::CannotAnswer;
    };
    let document = match get(&format!("{base}/.well-known/did.json")).await {
        Ok((_, Some(document))) => document,
        _ => return Listing::CannotAnswer,
    };
    let key_id = format!("{did}#freeq");
    let current = document
        .get("verificationMethod")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .find(|m| {
            m.get("id")
                .and_then(|id| id.as_str())
                .is_some_and(|id| id == key_id || id == "#freeq")
        })
        .and_then(|m| m.get("publicKeyMultibase")?.as_str())
        .and_then(
            |mb| match freeq_sdk::crypto::PublicKey::from_multibase(mb) {
                Ok(freeq_sdk::crypto::PublicKey::Ed25519(key)) => Some(key),
                _ => None,
            },
        );
    let Some(current) = current else {
        return Listing::CannotAnswer;
    };
    if freeq_sdk::act::derive_kid_bytes(current.as_bytes()) == kid {
        return Listing::Listed(current, None);
    }

    // Not its current key: the host's own list holds every key it has had.
    let mut url = match url::Url::parse(&base) {
        Ok(url) => url,
        Err(_) => return Listing::CannotAnswer,
    };
    url.path_segments_mut().expect("an http base URL").extend([
        "api",
        "v1",
        "signing-keys",
        did,
        kid,
    ]);
    match get(url.as_str()).await {
        Ok((404, _)) => Listing::NotListed,
        Ok((_, Some(body))) => {
            use base64::Engine;
            let key = body
                .get("public_key")
                .and_then(|v| v.as_str())
                .and_then(|b64| {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(b64)
                        .ok()
                })
                .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
                .and_then(|bytes| ed25519_dalek::VerifyingKey::from_bytes(&bytes).ok());
            let date = |name: &str| body.get(name).and_then(|v| v.as_f64()).map(|v| v as i64);
            match key {
                Some(key) if freeq_sdk::act::derive_kid_bytes(key.as_bytes()) == kid => {
                    Listing::Listed(key, stopped_at(date("removed_at"), date("expires_at")))
                }
                // An answer that is not the key asked for lists nothing.
                _ => Listing::NotListed,
            }
        }
        _ => Listing::CannotAnswer,
    }
}

/// `did:web:` host names pointed at a local stub, for tests.
#[cfg(test)]
static TEST_HOSTS: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Point `host`'s did:web lookups at `base` (a loopback stub) in tests.
#[cfg(test)]
pub(crate) fn point_host_at(host: &str, base: &str) {
    TEST_HOSTS.lock().insert(host.to_string(), base.to_string());
}

/// Serve a referee's own host on loopback: its document with `current` as
/// `#freeq`, and its key route answering for every key in `listed`. Returns
/// the base URL and points `did`'s host at it.
#[cfg(test)]
pub(crate) async fn stub_referee(
    did: &str,
    current: &ed25519_dalek::VerifyingKey,
    listed: Vec<ed25519_dalek::VerifyingKey>,
) -> String {
    stub_referee_retiring(did, current, listed, &[]).await
}

/// [`stub_referee`], with each key in `retired` answered with that
/// `removed_at` (unix seconds).
#[cfg(test)]
pub(crate) async fn stub_referee_retiring(
    did: &str,
    current: &ed25519_dalek::VerifyingKey,
    listed: Vec<ed25519_dalek::VerifyingKey>,
    retired: &[(ed25519_dalek::VerifyingKey, i64)],
) -> String {
    use axum::routing::get;
    use base64::Engine;
    let doc = serde_json::json!({
        "id": did,
        "verificationMethod": [{
            "id": format!("{did}#freeq"),
            "type": "Multikey",
            "controller": did,
            "publicKeyMultibase":
                freeq_sdk::crypto::PublicKey::Ed25519(*current).to_multibase(),
        }],
    });
    let keys: HashMap<String, serde_json::Value> = listed
        .iter()
        .map(|key| {
            let removed_at = retired.iter().find(|(k, _)| k == key).map(|(_, at)| *at);
            (
                freeq_sdk::act::derive_kid_bytes(key.as_bytes()),
                serde_json::json!({
                    "public_key":
                        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.as_bytes()),
                    "removed_at": removed_at,
                }),
            )
        })
        .collect();
    let app = axum::Router::new()
        .route(
            "/.well-known/did.json",
            get(move || {
                let doc = doc.clone();
                async move { axum::Json(doc) }
            }),
        )
        .route(
            "/api/v1/signing-keys/{did}/{kid}",
            get(
                move |axum::extract::Path((_, kid)): axum::extract::Path<(String, String)>| {
                    let found = keys.get(&kid).cloned();
                    async move {
                        match found {
                            Some(answer) => Ok(axum::Json(answer)),
                            None => Err(axum::http::StatusCode::NOT_FOUND),
                        }
                    }
                },
            ),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    point_host_at(did.strip_prefix("did:web:").expect("a did:web name"), &base);
    base
}

/// Serve `document` as a host's `/.well-known/did.json` and nothing else, and
/// point `did`'s host at it, in tests.
#[cfg(test)]
pub(crate) async fn stub_document(did: &str, document: serde_json::Value) {
    let app = axum::Router::new().route(
        "/.well-known/did.json",
        axum::routing::get(move || {
            let document = document.clone();
            async move { axum::Json(document) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    point_host_at(did.strip_prefix("did:web:").expect("a did:web name"), &base);
}

/// Wait until the referee's answer for `(did, kid)` is in, in tests.
#[cfg(test)]
pub(crate) async fn settled(did: &str, kid: &str) {
    let pair = (did.to_string(), kid.to_string());
    for _ in 0..200 {
        if !ASKING.lock().contains(&pair) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the referee never answered for {did} {kid}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn kid_of(key: &ed25519_dalek::VerifyingKey) -> String {
        freeq_sdk::act::derive_kid_bytes(key.as_bytes())
    }

    #[tokio::test]
    async fn the_current_key_in_the_referees_document_is_listed() {
        let did = "did:web:referee-current.example";
        let key = SigningKey::from_bytes(&[201u8; 32]).verifying_key();
        stub_referee(did, &key, vec![key]).await;
        assert_eq!(fetch(did, &kid_of(&key)).await, Listing::Listed(key, None));
    }

    #[tokio::test]
    async fn a_retired_key_is_listed_by_the_referees_own_key_route() {
        let did = "did:web:referee-retired.example";
        let current = SigningKey::from_bytes(&[202u8; 32]).verifying_key();
        let old = SigningKey::from_bytes(&[203u8; 32]).verifying_key();
        stub_referee(did, &current, vec![current, old]).await;
        assert_eq!(fetch(did, &kid_of(&old)).await, Listing::Listed(old, None));
    }

    #[tokio::test]
    async fn a_key_the_referee_does_not_list_is_not_listed() {
        let did = "did:web:referee-unlisted.example";
        let current = SigningKey::from_bytes(&[204u8; 32]).verifying_key();
        let stranger = SigningKey::from_bytes(&[205u8; 32]).verifying_key();
        stub_referee(did, &current, vec![current]).await;
        assert_eq!(fetch(did, &kid_of(&stranger)).await, Listing::NotListed);
    }

    #[tokio::test]
    async fn a_retired_key_is_listed_with_when_it_was_retired() {
        let did = "did:web:referee-retired-fetch.example";
        let current = SigningKey::from_bytes(&[206u8; 32]).verifying_key();
        let old = SigningKey::from_bytes(&[207u8; 32]).verifying_key();
        stub_referee_retiring(did, &current, vec![current, old], &[(old, 1_700_000_000)]).await;
        assert_eq!(
            fetch(did, &kid_of(&old)).await,
            Listing::Listed(old, Some(1_700_000_000))
        );
        assert!(signed_while_live(
            Some(1_700_000_000),
            "01H0000000AAAAAAAAAAAAAAAA"
        ));
        assert!(!signed_while_live(
            Some(1_700_000_000),
            &freeq_sdk::chatsig::new_event_id()
        ));
        assert!(signed_while_live(None, &freeq_sdk::chatsig::new_event_id()));
    }

    #[test]
    fn what_cannot_answer_is_remembered_stays_bounded() {
        let state = crate::server::test_state_with_db();
        for n in 0..(MAX_CANNOT_ANSWER + 50) {
            remember_cannot_answer(&state, ("did:web:flood.example".into(), format!("kid{n}")));
        }
        assert!(CANNOT_ANSWER.lock().len() <= MAX_CANNOT_ANSWER);
    }

    #[tokio::test]
    async fn a_host_that_cannot_be_reached_cannot_answer() {
        // Nothing listens on port 9 of loopback.
        point_host_at("referee-down.example", "http://127.0.0.1:9");
        assert_eq!(
            fetch("did:web:referee-down.example", "somekid").await,
            Listing::CannotAnswer
        );
        assert_eq!(
            fetch("did:web:freeq:path", "somekid").await,
            Listing::CannotAnswer
        );
    }
}

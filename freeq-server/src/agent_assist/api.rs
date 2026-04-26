//! HTTP routes for the agent assistance interface.
//!
//! Handlers are intentionally thin: extract the [`Caller`], call the
//! deterministic tool, wrap the resulting [`FactBundle`] in an
//! [`AssistResponse`] envelope. This split is what lets the next PR
//! drop in an LLM summarizer between tool and envelope without
//! reshaping any handler.

use super::caller;
use super::tools;
use super::types::*;
use crate::server::SharedState;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use std::sync::Arc;

/// Build the agent-assist router. Merged into the main app router by
/// [`crate::web::router`].
pub fn routes() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/.well-known/agent.json", get(get_discovery))
        .route("/agent/tools/validate_client_config", post(post_validate_client_config))
        .route("/agent/tools/diagnose_message_ordering", post(post_diagnose_message_ordering))
        .route("/agent/tools/diagnose_sync", post(post_diagnose_sync))
}

/// Capabilities advertised by the discovery endpoint. Kept in lock-step
/// with the tool routes above so agents can rely on this as truth.
const CAPABILITIES: &[&str] = &[
    "validate_client_config",
    "diagnose_message_ordering",
    "diagnose_sync",
];

// ─── Discovery ───────────────────────────────────────────────────────────

async fn get_discovery() -> impl IntoResponse {
    Json(AgentDiscovery {
        service: "Freeq",
        version: env!("CARGO_PKG_VERSION"),
        description:
            "Agent-facing assistance interface for Freeq client validation and \
             diagnostic queries. Returns conclusions, never raw state.",
        assistance_endpoint: "/agent/tools",
        capabilities: CAPABILITIES.to_vec(),
        auth: AgentDiscoveryAuth {
            required: false,
            methods: vec!["bearer"],
        },
    })
}

// ─── Tool handlers ───────────────────────────────────────────────────────

async fn post_validate_client_config(
    State(state): State<Arc<SharedState>>,
    headers: HeaderMap,
    Json(input): Json<ValidateClientConfigInput>,
) -> impl IntoResponse {
    let caller = caller::extract(&headers, &state);
    let request_id = new_request_id();
    let bundle = tools::validate_client_config(&input);
    log_audit("validate_client_config", &request_id, &caller, &bundle);
    Json(envelope(request_id, bundle, &caller)).into_response()
}

async fn post_diagnose_message_ordering(
    State(state): State<Arc<SharedState>>,
    headers: HeaderMap,
    Json(input): Json<DiagnoseMessageOrderingInput>,
) -> impl IntoResponse {
    let caller = caller::extract(&headers, &state);
    let request_id = new_request_id();
    let bundle = tools::diagnose_message_ordering(&input, &caller, &state);
    log_audit("diagnose_message_ordering", &request_id, &caller, &bundle);
    Json(envelope(request_id, bundle, &caller)).into_response()
}

async fn post_diagnose_sync(
    State(state): State<Arc<SharedState>>,
    headers: HeaderMap,
    Json(input): Json<DiagnoseSyncInput>,
) -> impl IntoResponse {
    let caller = caller::extract(&headers, &state);
    let request_id = new_request_id();
    let bundle = tools::diagnose_sync(&input, &caller, &state);
    log_audit("diagnose_sync", &request_id, &caller, &bundle);
    Json(envelope(request_id, bundle, &caller)).into_response()
}

// ─── Envelope assembly + final disclosure check ──────────────────────────

/// Final guard between a tool's [`FactBundle`] and the wire response.
///
/// Tools perform their own per-channel permission checks at the start,
/// so this is a *narrow* defense-in-depth pass: it catches the one
/// failure mode the abstract `caller.level` is sufficient to detect —
/// a bundle marked server-operator-only being returned to a non-admin
/// caller. Per-channel disclosure (ChannelMember / ChannelOperator)
/// can't be re-checked here because we don't carry the channel
/// context into the envelope; that's the tool's responsibility.
fn envelope(request_id: String, bundle: FactBundle, caller: &Caller) -> AssistResponse {
    let admin_only = matches!(bundle.min_disclosure, DisclosureLevel::ServerOperator);
    let caller_is_admin = matches!(caller.level, DisclosureLevel::ServerOperator);
    if admin_only && !caller_is_admin {
        let denied = tools::permission_denied(
            "DISCLOSURE_FILTER_BLOCKED",
            "Tool returned admin-only facts to a non-admin caller; redacted.",
            DisclosureLevel::ServerOperator,
        );
        return AssistResponse::from_bundle(request_id, denied);
    }
    AssistResponse::from_bundle(request_id, bundle)
}

/// Audit log for every assistance request. Per spec §16, all
/// diagnostic requests must be auditable.
fn log_audit(tool: &str, request_id: &str, caller: &Caller, bundle: &FactBundle) {
    tracing::info!(
        target: "agent_assist::audit",
        tool,
        request_id,
        caller_did = caller.did.as_deref().unwrap_or("anonymous"),
        caller_level = ?caller.level,
        ok = bundle.ok,
        code = %bundle.code,
        "agent assistance request",
    );
}

fn new_request_id() -> String {
    // Compact, sortable, no extra crate: timestamp_ns + a 4-byte
    // PRNG slice from the system clock's sub-nanosecond jitter.
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("req_{}{:04x}", now.as_secs(), now.subsec_nanos() & 0xFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_assist::types::Confidence;

    fn bundle_at(level: DisclosureLevel) -> FactBundle {
        FactBundle {
            ok: true,
            code: "TEST".into(),
            summary: "ok".into(),
            confidence: Confidence::High,
            safe_facts: vec!["fact one".into()],
            suggested_fixes: vec![],
            redactions: vec![],
            followups: vec![],
            min_disclosure: level,
        }
    }

    #[test]
    fn envelope_blocks_admin_only_bundle_for_non_admin() {
        let caller = Caller::anonymous();
        let resp = envelope("req".into(), bundle_at(DisclosureLevel::ServerOperator), &caller);
        assert!(!resp.ok);
        assert_eq!(resp.diagnosis.code, "DISCLOSURE_FILTER_BLOCKED");
        // Defense-in-depth: the original safe_facts must not appear.
        assert!(!resp.safe_facts.iter().any(|f| f.contains("fact one")));
    }

    #[test]
    fn envelope_passes_admin_only_bundle_for_admin() {
        let caller = Caller {
            did: Some("did:plc:admin".into()),
            session_id: Some("s1".into()),
            level: DisclosureLevel::ServerOperator,
        };
        let resp = envelope("req".into(), bundle_at(DisclosureLevel::ServerOperator), &caller);
        assert!(resp.ok);
        assert_eq!(resp.diagnosis.code, "TEST");
        assert_eq!(resp.safe_facts, vec!["fact one".to_string()]);
    }

    #[test]
    fn envelope_passes_account_level_to_anonymous_trusting_tool_check() {
        // Per-channel checks live in the tool. The envelope intentionally
        // does NOT re-check ChannelMember/ChannelOperator because it
        // doesn't carry the channel context. A tool that forgot its
        // upfront check would slip through here — but that's why every
        // tool is responsible for its own permission gate.
        let caller = Caller::anonymous();
        let resp = envelope("req".into(), bundle_at(DisclosureLevel::ChannelMember), &caller);
        assert!(resp.ok);
    }
}

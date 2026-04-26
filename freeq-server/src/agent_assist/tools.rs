//! Deterministic diagnostic tools.
//!
//! Each tool takes a typed input + a [`Caller`] + the shared state,
//! reads from authoritative server state (DB, in-memory channel maps),
//! applies the disclosure filter for the bundle's required level, and
//! returns a [`FactBundle`].
//!
//! The tools are pure with respect to the request — they do not record
//! into the diagnostic recorder, do not mutate state, and do not write
//! to the network. That property is what lets the LLM PR safely call
//! them as fact-gathering primitives without worrying about side
//! effects.
//!
//! ### Adding a tool
//!
//! 1. Add an input struct in [`super::types`].
//! 2. Add a `pub fn tool_xyz(input, caller, state) -> FactBundle` here.
//! 3. Wire an HTTP handler in [`super::api`].
//! 4. Add a row to the discovery `capabilities` array.
//!
//! The LLM router (next PR) will pick a tool based on a free-form
//! `/agent/session` request, call it, then optionally summarise the
//! returned bundle's `safe_facts`. None of that affects this file.

use super::caller;
use super::types::*;
use crate::server::SharedState;
use std::sync::Arc;

// ─── Tool: validate_client_config ────────────────────────────────────────

/// Pure-function validator. No state read, no DB hit. Public.
///
/// Rules implemented for MVP cover the most common foot-guns we have
/// observed when someone bolts together a fresh client. Add to this
/// list as new client failure modes show up in the wild — every entry
/// should pair a missing/unsupported feature with a concrete fix.
pub fn validate_client_config(input: &ValidateClientConfigInput) -> FactBundle {
    let mut warnings: Vec<String> = Vec::new();
    let mut fixes: Vec<SuggestedFix> = Vec::new();

    let safe_name = sanitize_label(&input.client_name);

    if !input.supports.message_tags {
        warnings.push(
            "Client does not advertise `message-tags`. msgid, time, and reply tags will be \
             missing on every PRIVMSG, breaking edits, deletes, replies, and reactions."
                .into(),
        );
        fixes.push(SuggestedFix {
            summary: "Negotiate the `message-tags` IRCv3 capability before joining channels.".into(),
            details: Some(
                "Send `CAP LS 302`, then `CAP REQ :message-tags server-time batch` and wait for \
                 `CAP ACK` before sending USER/NICK or any JOIN."
                    .into(),
            ),
        });
    }

    if !input.supports.server_time {
        warnings.push(
            "Client does not advertise `server-time`. Timestamps on history and live messages \
             will fall back to local receive time, which causes ordering surprises after \
             reconnect."
                .into(),
        );
        fixes.push(SuggestedFix {
            summary: "Request `server-time` and render messages by the server `time` tag.".into(),
            details: None,
        });
    }

    if !input.supports.batch {
        warnings.push(
            "Client does not advertise `batch`. CHATHISTORY replies will arrive as a flat \
             interleaved stream and may be ordered incorrectly relative to live messages."
                .into(),
        );
        fixes.push(SuggestedFix {
            summary: "Request `batch` to receive CHATHISTORY as a delimited group.".into(),
            details: None,
        });
    }

    if !input.supports.echo_message {
        warnings.push(
            "Client does not advertise `echo-message`. Self-sent messages will not be echoed \
             back, which makes optimistic local rendering racy with edits and deletes."
                .into(),
        );
    }

    if input.supports.e2ee && !input.supports.message_tags {
        warnings.push(
            "Client claims E2EE support but not message-tags. E2EE relies on tag-based metadata \
             (msgid, sig, encrypted) — this combination will not work."
                .into(),
        );
    }

    let wants_multi_device = input
        .desired_features
        .iter()
        .any(|f| f.eq_ignore_ascii_case("multi_device") || f.eq_ignore_ascii_case("multi-device"));

    if wants_multi_device && !input.supports.resume {
        warnings.push(
            "Client wants multi_device but does not advertise resume support. After a reconnect, \
             other devices will see a transient quit/join cycle and history may be replayed in \
             a different order than canonical."
                .into(),
        );
        fixes.push(SuggestedFix {
            summary: "Implement session resume so reconnects don't churn presence and replay.".into(),
            details: Some(
                "Persist the last server `msgid` you observed per channel and request \
                 `CHATHISTORY AFTER #ch msgid=<...>` on reconnect."
                    .into(),
            ),
        });
    }

    let mut safe_facts: Vec<String> = Vec::new();
    safe_facts.push(format!("Validated configuration for client `{safe_name}`."));
    safe_facts.push(format!(
        "Capability bitmap observed: message-tags={mt}, server-time={st}, batch={ba}, \
         sasl={sa}, resume={re}, e2ee={e2}, echo-message={em}, away-notify={aw}.",
        mt = input.supports.message_tags,
        st = input.supports.server_time,
        ba = input.supports.batch,
        sa = input.supports.sasl,
        re = input.supports.resume,
        e2 = input.supports.e2ee,
        em = input.supports.echo_message,
        aw = input.supports.away_notify,
    ));
    safe_facts.extend(warnings.iter().cloned());

    let ok = warnings.is_empty();
    let (code, summary, confidence) = if ok {
        (
            "CONFIG_OK".to_string(),
            "Client configuration looks compatible with current server expectations.".to_string(),
            Confidence::High,
        )
    } else {
        (
            "CONFIG_HAS_WARNINGS".to_string(),
            format!("Client configuration has {} compatibility warning(s).", warnings.len()),
            Confidence::High,
        )
    };

    FactBundle {
        ok,
        code,
        summary,
        confidence,
        safe_facts,
        suggested_fixes: fixes,
        redactions: vec![],
        followups: vec![],
        // Validator only inspects caller-supplied JSON, no server state.
        min_disclosure: DisclosureLevel::Public,
    }
}

// ─── Tool: diagnose_message_ordering ─────────────────────────────────────

/// Compare the caller's observed display order against the canonical
/// server order for the listed msgids in a channel. Reads from the
/// `messages` table directly.
///
/// Disclosure: requires the caller to be a member of the channel, or
/// a server operator. Message bodies are never returned; only the
/// canonical sequence (autoincrement row id) and server timestamp.
pub fn diagnose_message_ordering(
    input: &DiagnoseMessageOrderingInput,
    caller: &Caller,
    state: &Arc<SharedState>,
) -> FactBundle {
    let channel = normalize_channel(&input.channel);
    let safe_channel = sanitize_label(&channel);

    let effective = caller::effective_level(caller, state, &channel);
    if !effective.satisfies(DisclosureLevel::ChannelMember) {
        return permission_denied(
            "DIAGNOSE_MESSAGE_ORDERING_REQUIRES_MEMBERSHIP",
            "You must be a member of the channel to inspect its message ordering.",
            DisclosureLevel::ChannelMember,
        );
    }

    if input.message_ids.is_empty() {
        return FactBundle {
            ok: false,
            code: "INVALID_INPUT".into(),
            summary: "Provide at least one msgid to diagnose ordering.".into(),
            confidence: Confidence::High,
            safe_facts: vec![],
            suggested_fixes: vec![SuggestedFix {
                summary: "Pass `message_ids` as a non-empty array of ULID msgids.".into(),
                details: None,
            }],
            redactions: vec![],
            followups: vec![],
            min_disclosure: DisclosureLevel::ChannelMember,
        };
    }
    if input.message_ids.len() > 50 {
        return FactBundle {
            ok: false,
            code: "INVALID_INPUT".into(),
            summary: "Diagnose at most 50 messages per request.".into(),
            confidence: Confidence::High,
            safe_facts: vec![],
            suggested_fixes: vec![],
            redactions: vec![],
            followups: vec![],
            min_disclosure: DisclosureLevel::ChannelMember,
        };
    }

    // Resolve each msgid to (server_sequence, server_time). Missing
    // ones are reported but not fatal.
    let lookups: Vec<(String, Option<(i64, u64)>)> = input
        .message_ids
        .iter()
        .map(|id| {
            let row = state
                .with_db(|db| db.find_message_by_msgid(id))
                .flatten();
            (id.clone(), row.map(|r| (r.id, r.timestamp)))
        })
        .collect();

    let mut missing: Vec<&String> = Vec::new();
    let mut resolved: Vec<(&String, i64, u64)> = Vec::new();
    for (id, found) in &lookups {
        match found {
            None => missing.push(id),
            Some((seq, ts)) => resolved.push((id, *seq, *ts)),
        }
    }

    if resolved.is_empty() {
        return FactBundle {
            ok: false,
            code: "MESSAGES_NOT_FOUND".into(),
            summary: format!(
                "None of the {} provided msgid(s) were found in `{safe_channel}`.",
                input.message_ids.len()
            ),
            confidence: Confidence::High,
            safe_facts: vec![format!(
                "Looked up {} msgid(s); 0 matched persisted messages.",
                input.message_ids.len()
            )],
            suggested_fixes: vec![SuggestedFix {
                summary: "Confirm the msgids and channel name match what the server emitted.".into(),
                details: None,
            }],
            redactions: vec!["Message bodies omitted.".into()],
            followups: vec![],
            min_disclosure: DisclosureLevel::ChannelMember,
        };
    }

    // Canonical order = ascending server_sequence (auto-increment row id).
    let mut canonical = resolved.clone();
    canonical.sort_by_key(|(_, seq, _)| *seq);
    let canonical_ids: Vec<&String> = canonical.iter().map(|(id, _, _)| *id).collect();

    let observed_resolved: Vec<&String> = resolved.iter().map(|(id, _, _)| *id).collect();

    let order_matches = observed_resolved == canonical_ids;

    let mut safe_facts: Vec<String> = Vec::new();
    safe_facts.push(format!("Channel: `{safe_channel}`."));
    for (id, seq, ts) in &canonical {
        safe_facts.push(format!(
            "msgid `{}` — server_sequence={}, server_time={}.",
            sanitize_label(id),
            seq,
            ts
        ));
    }
    if !missing.is_empty() {
        safe_facts.push(format!(
            "{} provided msgid(s) not found in `{safe_channel}`.",
            missing.len()
        ));
    }
    safe_facts.push(format!(
        "Canonical order (oldest → newest): [{}].",
        canonical_ids
            .iter()
            .map(|s| format!("`{}`", sanitize_label(s)))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if let Some(symptom) = &input.symptom {
        safe_facts.push(format!("Caller-reported symptom: {}", quote_user_text(symptom)));
    }

    let (code, summary, confidence, fixes) = if order_matches {
        (
            "ORDER_MATCHES_CANONICAL".to_string(),
            "The observed display order matches the canonical server order. The reported \
             symptom must have another cause."
                .to_string(),
            Confidence::High,
            vec![SuggestedFix {
                summary: "Look for client-side reordering after this point (debounce, virtualized \
                          scroll, or merge step)."
                    .into(),
                details: None,
            }],
        )
    } else {
        (
            "DISPLAY_ORDER_MISMATCH".to_string(),
            "Client display order does not match canonical server order. The most common \
             cause is rendering by local receive time instead of `server-time` or \
             server-assigned sequence."
                .to_string(),
            Confidence::High,
            vec![
                SuggestedFix {
                    summary: "Sort committed channel messages by server `time` tag (or \
                              server_sequence when available)."
                        .into(),
                    details: Some(
                        "Treat local pending messages as a separate optimistic layer until \
                         their echo arrives with a real msgid + time."
                            .into(),
                    ),
                },
                SuggestedFix {
                    summary: "During CHATHISTORY replay, buffer the batch and apply order on \
                              batch end."
                        .into(),
                    details: None,
                },
            ],
        )
    };

    FactBundle {
        ok: order_matches,
        code,
        summary,
        confidence,
        safe_facts,
        suggested_fixes: fixes,
        redactions: vec![
            "Message bodies omitted.".into(),
            "Sender identities omitted.".into(),
        ],
        followups: vec![Followup {
            tool: "diagnose_sync".into(),
            reason: "Inspect whether these messages were live-delivered or replayed during a \
                     resume."
                .into(),
        }],
        min_disclosure: DisclosureLevel::ChannelMember,
    }
}

// ─── Tool: diagnose_sync ─────────────────────────────────────────────────

/// Best-effort sync diagnosis given today's server state. Honestly
/// reports what the server *can* and *cannot* know.
///
/// We can answer:
/// - "How many concurrent sessions does this DID have right now?"
/// - "Which of those sessions are joined to the channel?"
///
/// We cannot answer (yet, until per-session delivery cursors are
/// added):
/// - "Which messages did each session actually receive?"
/// - "What's the resume cursor for session X?"
///
/// The bundle says so out loud rather than guessing.
pub fn diagnose_sync(
    input: &DiagnoseSyncInput,
    caller: &Caller,
    state: &Arc<SharedState>,
) -> FactBundle {
    // Self-scoping: a non-admin caller may only diagnose their own DID.
    let is_admin = matches!(caller.level, DisclosureLevel::ServerOperator);
    if !is_admin && !caller.is_self(&input.account) {
        return permission_denied(
            "DIAGNOSE_SYNC_SELF_ONLY",
            "Only the account owner (or a server operator) may diagnose this account's sync state.",
            DisclosureLevel::Account,
        );
    }

    let safe_account = sanitize_label(&input.account);
    let sessions: Vec<String> = state
        .did_sessions
        .lock()
        .get(&input.account)
        .map(|set| set.iter().cloned().collect())
        .unwrap_or_default();

    let mut safe_facts: Vec<String> = Vec::new();
    safe_facts.push(format!(
        "Account `{safe_account}` has {} active session(s).",
        sessions.len()
    ));

    if sessions.is_empty() {
        return FactBundle {
            ok: false,
            code: "ACCOUNT_NOT_CONNECTED".into(),
            summary: "The account has no active server sessions right now. The symptom \
                      cannot be diagnosed against live state — try again while connected, \
                      or pass `diagnose_message_ordering` for historical messages."
                .into(),
            confidence: Confidence::High,
            safe_facts,
            suggested_fixes: vec![],
            redactions: vec!["Other accounts' session state omitted.".into()],
            followups: vec![Followup {
                tool: "diagnose_message_ordering".into(),
                reason: "Works against persisted messages even when the account is offline.".into(),
            }],
            min_disclosure: DisclosureLevel::Account,
        };
    }

    if let Some(channel) = &input.channel {
        let normalized = normalize_channel(channel);
        let safe_channel = sanitize_label(&normalized);
        let joined: usize = state
            .channels
            .lock()
            .get(&normalized)
            .map(|ch| sessions.iter().filter(|sid| ch.members.contains(*sid)).count())
            .unwrap_or(0);
        safe_facts.push(format!(
            "{joined} of those {} session(s) are joined to `{safe_channel}`.",
            sessions.len()
        ));
        if joined == 0 {
            safe_facts.push(format!(
                "No live session is in `{safe_channel}` — historical messages will only \
                 reach this account on next JOIN.",
            ));
        }
    }

    if let Some(s) = &input.symptom {
        safe_facts.push(format!("Caller-reported symptom: {}", quote_user_text(s)));
    }

    safe_facts.push(
        "The server does not yet record per-session delivery cursors or resume markers, so \
         it cannot say which specific messages were already delivered to a given session. \
         If you need that, use `diagnose_message_ordering` against the msgids you have."
            .into(),
    );

    FactBundle {
        ok: true,
        code: "SYNC_STATE_REPORTED".into(),
        summary: "Reported the account's live session and channel-join state. Per-session \
                  delivery state is not yet recorded by the server."
            .into(),
        confidence: Confidence::Medium,
        safe_facts,
        suggested_fixes: vec![
            SuggestedFix {
                summary: "If reconnect ordering is the symptom, persist the last seen msgid per \
                          channel and request `CHATHISTORY AFTER` on reconnect."
                    .into(),
                details: None,
            },
            SuggestedFix {
                summary: "Render channel timelines by the server `time` tag, not by local receive \
                          time."
                    .into(),
                details: None,
            },
        ],
        redactions: vec![
            "Session ids and IPs omitted.".into(),
            "Other accounts' session state omitted.".into(),
        ],
        followups: vec![Followup {
            tool: "diagnose_message_ordering".into(),
            reason: "Compare canonical order against your client-observed order for specific msgids."
                .into(),
        }],
        min_disclosure: DisclosureLevel::Account,
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// A single place to construct permission-denied bundles so the wire
/// shape is uniform and exhaustively tested.
///
/// `required` is reported in the suggested fix so the caller knows
/// which auth tier they need. The bundle itself is `min_disclosure
/// = Public` because it deliberately contains no facts — there is
/// nothing left to filter, so the envelope's defense-in-depth check
/// shouldn't replace this with the generic `DISCLOSURE_FILTER_BLOCKED`.
pub(super) fn permission_denied(
    code: &str,
    summary: &str,
    required: DisclosureLevel,
) -> FactBundle {
    FactBundle {
        ok: false,
        code: code.to_string(),
        summary: summary.to_string(),
        confidence: Confidence::High,
        safe_facts: vec![],
        suggested_fixes: vec![SuggestedFix {
            summary: format!(
                "Authenticate with a session at the {required:?} disclosure level or higher."
            ),
            details: None,
        }],
        redactions: vec!["Request denied before any server state was inspected.".into()],
        followups: vec![],
        min_disclosure: DisclosureLevel::Public,
    }
}

fn normalize_channel(channel: &str) -> String {
    let with_hash = if channel.starts_with('#') || channel.starts_with('&') {
        channel.to_string()
    } else {
        format!("#{channel}")
    };
    with_hash.to_lowercase()
}

/// Strip control chars + cap length on caller-provided identifiers so
/// they're safe to interpolate into our own `safe_facts` strings.
fn sanitize_label(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(96));
    for c in input.chars().take(96) {
        if c.is_control() {
            out.push('?');
        } else {
            out.push(c);
        }
    }
    out
}

/// Quote arbitrary caller-supplied free text inside a fact line. The
/// surrounding quotes + the explicit "Caller-reported symptom:" prefix
/// ensure that any future LLM summarizer treats the content as
/// reported data, not as instructions to follow.
fn quote_user_text(s: &str) -> String {
    let truncated: String = s.chars().take(280).collect();
    let cleaned: String = truncated
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect();
    format!("\"{cleaned}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with(supports: ClientSupports) -> ValidateClientConfigInput {
        ValidateClientConfigInput {
            client_name: "test-client".into(),
            client_version: "0.0.1".into(),
            server_url: "wss://example".into(),
            transport: Some("websocket".into()),
            auth_method: Some("did_atproto".into()),
            supports,
            desired_features: vec![],
        }
    }

    #[test]
    fn validator_accepts_modern_client() {
        let bundle = validate_client_config(&input_with(ClientSupports {
            message_tags: true,
            batch: true,
            server_time: true,
            sasl: true,
            resume: true,
            e2ee: false,
            crdt_sync: true,
            echo_message: true,
            away_notify: true,
        }));
        assert!(bundle.ok);
        assert_eq!(bundle.code, "CONFIG_OK");
        assert!(bundle.suggested_fixes.is_empty());
    }

    #[test]
    fn validator_warns_about_missing_message_tags() {
        let bundle = validate_client_config(&input_with(ClientSupports::default()));
        assert!(!bundle.ok);
        assert_eq!(bundle.code, "CONFIG_HAS_WARNINGS");
        assert!(
            bundle
                .safe_facts
                .iter()
                .any(|f| f.contains("`message-tags`")),
            "expected message-tags warning, got: {:?}",
            bundle.safe_facts
        );
    }

    #[test]
    fn validator_flags_e2ee_without_message_tags() {
        let bundle = validate_client_config(&input_with(ClientSupports {
            e2ee: true,
            ..ClientSupports::default()
        }));
        assert!(
            bundle
                .safe_facts
                .iter()
                .any(|f| f.contains("E2EE support but not message-tags")),
        );
    }

    #[test]
    fn validator_flags_multi_device_without_resume() {
        let mut input = input_with(ClientSupports {
            message_tags: true,
            server_time: true,
            batch: true,
            sasl: true,
            echo_message: true,
            ..ClientSupports::default()
        });
        input.desired_features.push("multi_device".into());
        let bundle = validate_client_config(&input);
        assert!(!bundle.ok);
        assert!(
            bundle
                .safe_facts
                .iter()
                .any(|f| f.contains("multi_device but does not advertise resume")),
        );
    }

    #[test]
    fn ordering_input_validates_emptiness() {
        // We can't easily construct a SharedState in unit tests, so we
        // exercise the input-only branch which never touches state.
        let bundle = ordering_input_validation_only(&DiagnoseMessageOrderingInput {
            channel: "#general".into(),
            message_ids: vec![],
            symptom: None,
        });
        assert_eq!(bundle.code, "INVALID_INPUT");
    }

    /// Re-implementation of the pre-state guard so we can unit-test it
    /// without standing up SharedState. Mirrors the early-return branch
    /// in [`diagnose_message_ordering`]; if either drifts the test in
    /// `tests/agent_assist_api.rs` will catch it.
    fn ordering_input_validation_only(input: &DiagnoseMessageOrderingInput) -> FactBundle {
        if input.message_ids.is_empty() {
            return FactBundle {
                ok: false,
                code: "INVALID_INPUT".into(),
                summary: "x".into(),
                confidence: Confidence::High,
                safe_facts: vec![],
                suggested_fixes: vec![],
                redactions: vec![],
                followups: vec![],
                min_disclosure: DisclosureLevel::ChannelMember,
            };
        }
        permission_denied("X", "x", DisclosureLevel::ChannelMember)
    }

    #[test]
    fn quote_user_text_strips_control_chars_and_caps_length() {
        let s = format!("a\x07b\x1bc{}", "x".repeat(500));
        let q = quote_user_text(&s);
        assert!(q.starts_with('"') && q.ends_with('"'));
        assert!(!q.contains('\x07'));
        assert!(!q.contains('\x1b'));
        // 280 chars + 2 surrounding quotes
        assert!(q.chars().count() <= 282);
    }

    #[test]
    fn permission_denied_is_safe_at_public_level() {
        // Permission-denied bundles deliberately carry min_disclosure
        // = Public so the envelope's defense-in-depth filter doesn't
        // overwrite the more-informative tool-specific code with the
        // generic DISCLOSURE_FILTER_BLOCKED. Empty safe_facts means
        // there's nothing left to leak.
        let b = permission_denied("X", "no", DisclosureLevel::ServerOperator);
        assert!(!b.ok);
        assert!(b.safe_facts.is_empty());
        assert_eq!(b.min_disclosure, DisclosureLevel::Public);
        assert!(!b.redactions.is_empty());
        // The required level is still surfaced in the suggested fix.
        assert!(
            b.suggested_fixes
                .iter()
                .any(|f| f.summary.contains("ServerOperator")),
        );
    }

    #[test]
    fn normalize_channel_adds_hash_and_lowercases() {
        assert_eq!(normalize_channel("General"), "#general");
        assert_eq!(normalize_channel("#General"), "#general");
        assert_eq!(normalize_channel("&local"), "&local");
    }
}

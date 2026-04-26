# freeq-karma-bot

Classic IRC karma tracker (`nick++`, `nick--`, `!karma`, `!leaderboard`)
built on `@freeq/sdk` — and the simplest demonstration of the
**validation-driven** bot loop using the freeq agent assistance interface.

Every meaningful step of the bot's lifecycle is gated by a real call
to `/.well-known/agent.json` and the diagnostic tools it advertises.
The bot uses the answers to drive behaviour — not just decoration.

| Lifecycle step | Tool | What the bot does with the answer |
|---|---|---|
| Boot | `validate_client_config` | refuse to start if config has compatibility warnings (or downgrade) |
| Boot | discovery via `/.well-known/agent.json` | refuse if required tools aren't advertised |
| Join failure (`477`/`473`/`474`/`475`) | `diagnose_join_failure` | log structured cause + suggested fixes |
| Before each reply | `predict_message_outcome` | skip the send if the predictor says it would fail |
| Reconnect | `replay_missed_messages` | per-channel anchor → server reports the gap |

Every call (request + response) is appended to `validation.log` as
JSON Lines. See `sample-validation.jsonl` for a real captured session.

## Run

```bash
cd examples/karma-bot
npm install
npm start
```

Defaults: connects to `wss://irc.freeq.at/irc` as a guest with a
random nick, joins `#dev`, persists karma to `./karma.json`.

## Config

| Env var | Default | Notes |
|---|---|---|
| `FREEQ_SERVER` | `wss://irc.freeq.at/irc` | |
| `FREEQ_NICK` | `karma-XXXX` (random) | |
| `FREEQ_CHANNELS` | `#dev` | Comma-separated |
| `FREEQ_KARMA_FILE` | `./karma.json` | |
| `FREEQ_VALIDATION_LOG` | `./validation.log` | One JSON object per line |
| `FREEQ_BOT_DID` | (empty — anonymous) | When set, predict_message_outcome actually gates sends |
| `FREEQ_QUIT_AFTER_MS` | (run forever) | Auto-shutdown ms (smoke tests) |

## Use

In any joined channel:

```
alice++ — great work on the docs       → karma → alice: 1
bob++ chad++                           → karma → bob: 1, chad: 1
!karma alice                           → alice has 1 karma in #dev
!leaderboard                           → 🏆 #dev top karma → 1. alice: 1 | …
!karma-help                            → karma-bot: `nick++` / `nick--` …
```

Self-karma is silently rejected. URLs containing `++`-shaped substrings
don't trigger (regex anchors require word-boundary delimiters).

## Reading the validation log

```bash
tail -f validation.log | jq .
```

Every line is `{ts, stage, tool, request, response}`. Useful columns:

```bash
cat validation.log | jq -r '.stage + " → " + .tool + " (" + (.response.diagnosis.code // "no-diagnosis") + ")"' | sort | uniq -c
```

Sample output (5-second smoke run on prod):

```
   1 join_failure → diagnose_join_failure (DIAGNOSE_JOIN_FAILURE_SELF_ONLY)
   5 pre_send → predict_message_outcome (PREDICT_MESSAGE_OUTCOME_SELF_ONLY)
   1 preflight → discovery (no-diagnosis)
   1 preflight → validate_client_config (CONFIG_OK)
```

The two `*_SELF_ONLY` codes are the per-tool disclosure-filter
denying an anonymous caller — exactly the safety enforcement we want
to see firing. Set `FREEQ_BOT_DID=did:plc:yours` after authenticating
and those denials become real `PREDICTED_ACCEPTED` / `JOIN_DENIED` etc.
that the bot can act on.

## Design notes

- The bot opens the validation surface deliberately: every reply is
  gated by `predict_message_outcome` whether or not it'll actually
  succeed, so the bot's own logs can answer "why didn't I send?"
  even for an anonymous bot.
- `replay_missed_messages` requires a real anchor msgid — the bot
  tracks the last-seen msgid per channel from the SDK's `message`
  event and uses it on reconnect.
- `diagnose_join_failure` fires only on real join failures; on a
  successful JOIN no validation call is wasted.

## Source

`index.ts` — single-file bot (~250 lines). The `callTool()` helper
makes adding more validation hooks trivial:

```ts
await callTool('my_stage', 'inspect_my_session', { account: BOT_DID });
```

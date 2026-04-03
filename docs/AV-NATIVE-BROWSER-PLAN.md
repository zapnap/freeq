# AV Native + Browser Plan

**Goal:** Native client and browser can both join the same AV session through the SFU using iroh MoQ.

## Step 1: Redeploy staging with working SFU + port exposure

The SFU auth bug is already fixed (commit 6750e88) but staging hasn't been redeployed.
Also need to expose SFU port (4443 internal → 30443 external) via Miren node_port on blueyard-projects.

- [x] Update SessionIndicator.tsx to use /av/call (same origin, no separate SFU URL)
- [x] Fix Procfile to include --iroh flag (Miren uses Procfile over Dockerfile CMD)
- [x] Move call page + JS assets from av_sfu.rs to web.rs (/av/call, /av/assets/*)
- [x] Bind SFU QUIC (UDP) to web server's $PORT — no separate port needed
- [x] Deploy to blueyard-projects cluster
- [x] Verify SFU starts on :3000, call page loads at staging.freeq.at/av/call

## Step 2: Verify browser audio through SFU

- [ ] Open staging.freeq.at in two browsers
- [ ] Start voice session, both join
- [ ] Both click Audio → call page opens
- [ ] Verify audio flows between browsers through SFU

## Step 3: Refactor native client to connect via MoQ

- [ ] Add moq-native + moq-relay deps to freeq-av-client
- [ ] New `sfu` subcommand: connect to SFU endpoint via moq_native::Client
- [ ] Publish mic audio as MoQ broadcast: `{session}/{nick}/audio`
- [ ] Subscribe to other participants' broadcasts
- [ ] Keep iroh-live AudioBackend for capture/playback (just swap transport)

## Step 4: Wire session to SFU endpoint

- [ ] Server sends SFU endpoint URL (not Room ticket) when session starts
- [ ] Native client uses server-provided SFU address
- [ ] Test: native + browser in same session, both hear each other

## Step 5: Clean up

- [ ] Remove iroh-live Room code from server (av_media.rs)
- [ ] Remove Room-based commands from native client
- [ ] Update docs

# macOS App — Gap Analysis vs Web & iOS Clients

## Critical Bugs (P0)

1. **Members may not appear in detail panel** — Possible race: if NamesEnd arrives before channel is in `channels` array, `pendingNames` flush finds nothing. Need to ensure channel exists before flushing.

2. **Detail panel might not be visible** — `showDetailPanel` defaults true but user may not realize the right panel is the member list. Need visual affordance.

3. **CHATHISTORY batch target mismatch** — Server sends batch with target in different case than channel name. Need case-insensitive matching in `batchEnd`.

4. **Echo-message not handled** — Server echoes our own messages back with `echo-message` cap. SDK may send us our own messages, causing duplicates OR we never see our own messages if we rely on echo. Currently `sendMessage` doesn't add local echo — relies entirely on server echo.

---

## Product Gaps (vs Web App)

### Identity & Profiles
5. **No Bluesky avatars** — Web shows AT Protocol profile photos for authenticated users. macOS shows colored initials only.
6. **No display names** — Web shows Bluesky display name + handle. macOS shows IRC nick only.
7. **No verified badges** — Web shows ✓ for AT-authenticated users. macOS has no indicator.
8. **No DID resolution on members** — Web uses WHOIS + extended-join + account-notify to learn member DIDs. macOS doesn't track DIDs.
9. **User popover missing profile data** — Web popover shows avatar, display name, handle, DID, bio, E2EE safety number, WHOIS info. macOS member click just opens DM.

### Messages
10. **No link previews** — Web unfurls URLs with OpenGraph metadata (title, description, image). macOS shows raw URLs only.
11. **No Bluesky post embeds** — Web detects bsky.app links and renders rich embeds. macOS doesn't.
12. **No image lightbox** — Web has full-screen image viewer for image URLs. macOS doesn't.
13. **No signed message badge** — Web shows 🔒 badge for cryptographically signed messages (`+freeq.at/sig` tag). macOS ignores this tag.
14. **No E2EE encrypted badge** — Web shows encrypted indicator. macOS has no E2EE support at all.
15. **No message timestamps on hover** — Web shows full timestamp on hover. macOS always shows time but no hover detail.
16. **No message grouping collapse** — Web groups consecutive messages from same sender. macOS does this but doesn't collapse the header.

### Compose
17. **No emoji picker** — Web has searchable emoji grid with categories. macOS relies on system ⌘⌃Space only.
18. **No slash command autocomplete** — Web shows dropdown list of commands as you type /. macOS just processes them.
19. **No @mention autocomplete** — Web shows member dropdown when typing @nick. macOS has Tab completion but no popup.
20. **No :emoji: autocomplete** — Web shows emoji suggestions when typing :keyword. macOS doesn't.
21. **No file/image upload** — Web has drag-and-drop + file picker for image attachments. macOS has nothing.
22. **No format toolbar** — Web has bold/italic/code buttons. macOS relies on markdown syntax.
23. **No input history** — Web stores sent message history (Up/Down arrow to cycle). macOS only does Up for edit-last.
24. **No cross-post to Bluesky toggle** — Web has option to cross-post messages. macOS doesn't.

### Sidebar
25. **No favorites section** — Web has pinned/favorite channels at top. macOS doesn't.
26. **No muted channels** — Web allows muting channels (no notifications). macOS doesn't.
27. **No last message preview** — Web shows last message text + time in sidebar. macOS shows name only.
28. **No sidebar sorting** — Web sorts DMs by last activity. macOS sorts channels alphabetically only.
29. **No channel list browser** — Web has `/list` modal showing all available channels. macOS doesn't.

### Right Panel
30. **No channel settings panel** — Web has full settings: topic, modes, ops management, mod tools. macOS has nothing.
31. **No pinned messages bar** — Web shows pinned messages. macOS doesn't support pinning.

### Navigation
32. **No search** — Web has ⌘F message search. macOS has no search at all.
33. **No scroll-to-message** — Web can scroll to a specific message (from search, reply click). macOS can't.
34. **No thread view** — Web has thread/reply view. macOS shows flat reply indicators only.
35. **No bookmarks** — Web lets users bookmark messages. macOS doesn't.

### Connection
36. **No server MOTD** — Web shows Message of the Day banner. macOS ignores MOTD.
37. **No guest upgrade banner** — Web prompts guests to authenticate. macOS doesn't.
38. **No onboarding tour** — Web has first-time tutorial. macOS has nothing.

---

## Product Gaps (vs iOS App)

39. **No Bluesky avatar cache** — iOS has `AvatarCache` that prefetches all member avatars. macOS has none.
40. **No user profile sheet** — iOS has a dedicated profile sheet with avatar, Bluesky info, shared channels. macOS has a simple DM profile panel.
41. **No haptic feedback equivalent** — iOS uses haptics. macOS could use sound or animation.
42. **No background reconnect** — iOS handles scene phase (background/active). macOS doesn't.
43. **No notification badges on app icon** — iOS shows notification count. macOS has dock badge but untested.

---

## Technical Gaps

### SDK Integration
44. **WHOIS events not in FFI** — SDK has `Event::WhoisReply` but FFI maps it to catch-all Notice. Need dedicated FFI event.
45. **Account-notify not tracked** — SDK gets ACCOUNT messages but FFI doesn't expose them. Can't track member DIDs.
46. **Extended-join not used** — Server sends account (DID) in JOIN but FFI's Joined event doesn't include it.
47. **Message signing not integrated** — SDK supports `FreeqE2ee` for per-session ed25519 signing. macOS doesn't use it.
48. **E2EE not integrated** — `FreeqE2ee` provides session-based encryption. macOS doesn't initialize or use it.
49. **No MSGSIG registration** — After registration, SDK should register message signing key with server. macOS doesn't.
50. **Caps negotiation incomplete** — SDK requests caps but macOS doesn't verify echo-message behavior.

### Data Persistence
51. **No local message database** — Web uses IndexedDB. macOS keeps messages in memory only — lost on restart.
52. **No read position tracking** — Web tracks `lastReadMsgId` per channel. macOS doesn't.
53. **No settings persistence** — Compact mode, show join/part preferences not wired to actual behavior.

### Performance
54. **No message virtualization** — LazyVStack helps but no recycling of complex message views.
55. **No avatar caching** — If avatars are added, need disk + memory cache.
56. **No image proxy** — Link previews need a proxy to avoid CORS/privacy issues.

### Architecture
57. **No error recovery on FFI calls** — Many `try?` calls silently swallow errors.
58. **No structured logging** — No way to see what's happening in the app for debugging.
59. **`showJoinPart` setting not wired** — Setting exists but system messages always show.
60. **`compactMode` setting not wired** — Setting exists but layout doesn't change.

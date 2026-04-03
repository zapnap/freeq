# Freeq.WinUI — Feature Parity TODOs

Features present in `freeq-app` (web client) that are not yet implemented in `Freeq.WinUI`.
Grouped by priority. Web reference files are relative to `freeq-app/src/`.

---

## P0 — Core Messaging

- [x] **Message editing** — Up-arrow on empty ComposeBox loads last own message for editing. Edit bar shown with cancel. Submits `+draft/edit=<msgid>` PRIVMSG. Incoming edits update `Content`/`IsEdited` in-place via `INotifyPropertyChanged`. `(edited)` label shown.
  - `Controls/ComposeBox.xaml(.cs)`, `Controls/MessageList.xaml(.cs)`, `Services/IrcClient.cs`, `ViewModels/MainViewModel.cs`, `Models/MessageModel.cs`

- [x] **Message deletion** — Right-click → Delete sends `+draft/delete=<msgid>` TAGMSG. Incoming deletes set `IsDeleted=true` in-place. Deleted messages render as italic dimmed placeholder.
  - `Controls/MessageList.xaml(.cs)`, `Services/IrcClient.cs`, `ViewModels/MainViewModel.cs`

- [x] **Slash commands** — ComposeBox sends slash commands through `MainViewModel.HandleSlashCommand()`:
  `/me`, `/join`, `/part`, `/topic`, `/invite`, `/kick`, `/op`, `/deop`, `/voice`, `/mode`, `/msg`, `/whois`, `/away`, `/pins`, `/raw`, `/help`
  - `ViewModels/MainViewModel.cs`

- [x] **Nick autocomplete** — Tab key in ComposeBox cycles through nicks in the current channel that match the partial word before the caret. Appends `": "` when completing at line start (IRC convention).
  - `Controls/ComposeBox.xaml.cs`

- [x] **Compose box UX improvements**
  - Shift+Enter → inserts newline (`AcceptsReturn=True`; plain Enter still sends)
  - Up-arrow on empty input → begin editing last own message
  - Escape → cancel edit / close autocomplete
  - `Controls/ComposeBox.xaml(.cs)`

- [x] **Typing indicators** — Sends `@+typing=active TAGMSG` on keystroke; auto-stops after 10 s idle or on send. Inbound typing state tracked per-nick per-channel with 10 s auto-expiry. "X is typing…" shown above compose box.
  - `Controls/ComposeBox.xaml(.cs)`, `Services/IrcClient.cs`, `ViewModels/MainViewModel.cs`

---

## P1 — Channel & Navigation

- [x] **Channel browser** — Sidebar `+` flyout → Browse Channels: sends LIST, opens dialog with name/member count/topic, fuzzy filter, join from list.
  - `Controls/Sidebar.xaml.cs`, `Services/IrcClient.cs`, `ViewModels/MainViewModel.cs`, `Models/ChannelListEntry.cs`

- [x] **Channel creation** — Sidebar `+` flyout → New Channel: prompts for name, sends JOIN (server auto-creates).
  - `Controls/Sidebar.xaml`, `Controls/Sidebar.xaml.cs`

- [x] **Channel muting** — Right-click channel in sidebar → Mute/Unmute. Suppresses unread badges. Persisted to `%LOCALAPPDATA%\Freeq\settings.json`.
  - `Controls/Sidebar.xaml`, `Controls/Sidebar.xaml.cs`, `Services/AppSettings.cs`, `ViewModels/MainViewModel.cs`

- [x] **Pinned messages** — Pins button (📌) in TopBar opens dialog; sends `PINS #channel` to server; shows pinned message IDs, pinned-by, and timestamp.
  - `Controls/TopBar.xaml`, `Controls/TopBar.xaml.cs`, `Services/IrcClient.cs`, `Models/PinEntry.cs`

- [x] **Quick switcher** — Ctrl+K opens fuzzy-search dialog over all joined channels and DMs; Enter or double-click to navigate.
  - `MainWindow.xaml.cs`

- [x] **Keyboard channel switching**
  - Alt+1…9, Alt+0 → jump to nth channel in sidebar
  - Alt+Up / Alt+Down → previous/next channel
  - `MainWindow.xaml.cs`, `ViewModels/MainViewModel.cs`

- [x] **Channel topic setting** — TopBar topic text is now a clickable button; opens an edit dialog and sends `TOPIC #channel :new topic`.
  - `Controls/TopBar.xaml`, `Controls/TopBar.xaml.cs`

---

## P2 — User Experience

- [ ] **User profile popover** — Clicking a nick or avatar should open a popover showing: nick, DID (if authenticated), Bluesky handle, "Open on Bluesky" link, and op/voice actions for channel ops.
  - Web: `UserPopover.tsx`

- [ ] **Message reactions** — Support IRCv3 reactions: send `TAGMSG <target> +react=<emoji>`; render emoji reaction bubbles beneath messages with counts and click-to-react.
  - Web: `MessageList.tsx`, `EmojiPicker.tsx`, `store.ts`

- [ ] **Message search** — Ctrl+F opens a search dialog/panel. Query the server or local message cache; display results with channel context and click-to-jump.
  - Web: `SearchModal.tsx`

- [ ] **Message context menu** — Right-click (or long-press) on a message shows a context menu with: Copy text, Copy message ID, Edit (own messages), Delete (own/ops), React, Bookmark, Share to Bluesky.
  - Web: `MessageContextMenu.tsx`

- [ ] **Reconnect / identity-loss banner** — Show a dismissible banner at the top of the message list when: (a) reconnecting, (b) reconnected as guest after a previously authenticated session.
  - Web: `ReconnectBanner.tsx`

- [ ] **MOTD display** — Show server Message of the Day in the server buffer on connect (or in a dismissible banner). Suppress after first display per session.
  - Web: `MotdBanner.tsx`

- [ ] **Away status UI** — Add a button in the sidebar footer (or slash command) to toggle `/away [message]` / `/back`. Display away status on members in the member list.
  - Web: `SlashCommands.tsx`, `MemberList.tsx`

- [ ] **Toast / snackbar notifications** — Show brief in-app toasts for actions like "Message copied", "Channel joined", "Kicked user X", etc.
  - Web: `Toast.tsx`

- [ ] **Windows toast notifications** — Fire WinRT `ToastNotification` on @mention when the app is not the foreground window.
  - Web: `lib/notifications.ts` (browser Notification API equivalent)

- [ ] **Taskbar badge** — Show unread mention count on the taskbar icon using `BadgeNotification` / `BadgeUpdater`.

---

## P3 — Rich Media & Content

- [ ] **File / image upload** — Support drag-and-drop files and Ctrl+V paste of images into ComposeBox. Upload to server, send URL in message.
  - Web: `ComposeBox.tsx`, `FileDropOverlay.tsx`

- [ ] **Image lightbox** — Click any inline image to open it full-size in an overlay with zoom support.
  - Web: `ImageLightbox.tsx`

- [ ] **Link previews** — Fetch OpenGraph metadata for URLs in messages (via server proxy) and render a compact preview card below the message.
  - Web: `LinkPreview.tsx`

- [ ] **Bluesky post embeds** — Detect Bluesky post URLs (`bsky.app/profile/…/post/…`) in messages and render an embedded post card.
  - Web: `BlueskyEmbed.tsx`

- [ ] **Markdown rendering** — Parse and render `**bold**`, `*italic*`, `` `code` ``, `~~strikethrough~~`, ` ```code blocks``` `, and `[links](url)` in message content.
  - Web: `MarkdownRenderer.tsx`

- [ ] **Rich text compose toolbar** — Add a formatting toolbar above/below ComposeBox with Bold, Italic, Code, and Link buttons that wrap selected text.
  - Web: `FormatToolbar.tsx`

- [ ] **Audio / video / voice message playback** — Detect audio/video file URLs and render an inline player instead of a plain link.
  - Web: `MessageList.tsx`

---

## P4 — Settings & Preferences

- [ ] **Settings panel** — Wire the existing gear icon in TopBar to open a real settings dialog containing all preferences below.
  - Web: `SettingsPanel.tsx`

- [ ] **Theme toggle** — Add light/dark/system theme option. Currently the app is dark-only.
  - Web: `SettingsPanel.tsx`, `store.ts`

- [ ] **Message density** — Cozy (large avatars, extra spacing) / Default / Compact (dense, no avatars) display modes.
  - Web: `SettingsPanel.tsx`, `store.ts`

- [ ] **Show/hide join-part messages** — Toggle system messages (join, part, quit) per-channel or globally.
  - Web: `SettingsPanel.tsx`, `store.ts`

- [ ] **External media loading** — Toggle to disable automatic image/media loading (privacy / bandwidth control).
  - Web: `SettingsPanel.tsx`, `store.ts`

- [ ] **Notification preferences** — Enable/disable Windows notifications; enable/disable notification sounds per event type.
  - Web: `SettingsPanel.tsx`

- [ ] **Keyboard shortcuts reference** — Ctrl+/ opens a help overlay listing all keyboard shortcuts.
  - Web: `KeyboardShortcuts.tsx`

---

## P5 — Moderation & Advanced IRC

- [ ] **Channel settings panel** — Ops can open a panel (from TopBar) to manage: topic, modes, join policy, bans, invites.
  - Web: `ChannelSettingsPanel.tsx`

- [ ] **Audit timeline** — View a chronological log of governance events (kicks, bans, mode changes, pins) for a channel.
  - Web: `AuditTimeline.tsx`

- [ ] **Ban / invite management** — UI to view, add, and remove channel bans (`+b`) and invite exceptions (`+I`).
  - Web: `ChannelSettingsPanel.tsx`

- [ ] **Message bookmarks** — Bookmark any message; view bookmarks in a panel. Store bookmarks locally.
  - Web: `BookmarksPanel.tsx`, `MessageContextMenu.tsx`

- [ ] **Coordination cards** — Render structured task/event messages (those carrying coordination metadata) as rich cards instead of plain text.
  - Web: `CoordinationCards.tsx`

- [ ] **E2EE indicators** — Show a lock icon in TopBar for encrypted channels (+E mode). Show E2EE status in DM headers. Offer safety number comparison in user popover.
  - Web: `TopBar.tsx`, `UserPopover.tsx`

---

## Out of Scope (web/mobile-only)

These features exist in `freeq-app` but don't apply to a native Windows desktop client:

- PWA / service worker / install prompt
- Mobile responsive layout and swipe gestures
- Virtual keyboard avoidance
- Browser Notification API (replaced by WinRT toasts above)

# TypeScript SDK

The freeq TypeScript SDK (`@freeq/sdk`) lets you build IRC clients, bots, and integrations in TypeScript or JavaScript. It handles the IRC protocol, AT Protocol authentication, IRCv3 capabilities, and end-to-end encryption — so you can focus on your application logic.

The SDK is framework-agnostic. No React, no Zustand, no DOM dependencies. Use it in browsers, Node.js, Deno, or Bun.

## Installation

```bash
npm install @freeq/sdk
```

## Quick Start

Connect to a freeq server and start sending messages in under 20 lines:

```typescript
import { FreeqClient } from '@freeq/sdk';

const client = new FreeqClient({
  url: 'wss://irc.freeq.at/irc',
  nick: 'mybot',
  channels: ['#general'],
});

client.on('message', (channel, msg) => {
  console.log(`[${channel}] ${msg.from}: ${msg.text}`);

  // Echo bot
  if (!msg.isSelf && msg.text.startsWith('!echo ')) {
    client.sendMessage(channel, msg.text.slice(6));
  }
});

client.on('ready', () => {
  console.log(`Connected as ${client.nick}`);
});

client.connect();
```

## Authentication

### Guest Mode

No credentials needed — just connect:

```typescript
const client = new FreeqClient({
  url: 'wss://irc.freeq.at/irc',
  nick: 'guest-bot',
});
client.connect();
```

### AT Protocol (Bluesky) Identity

Authenticate with a DID to get a persistent identity, persistent channel memberships, DM history, and E2EE:

```typescript
const client = new FreeqClient({
  url: 'wss://irc.freeq.at/irc',
  nick: 'myhandle.bsky.social',
  sasl: {
    token: oauthToken,        // from AT Protocol OAuth flow
    did: 'did:plc:abc123',
    pdsUrl: 'https://bsky.social',
    method: 'pds-session',
  },
});

client.on('authenticated', (did, message) => {
  console.log(`Authenticated as ${did}`);
});

client.connect();
```

### Broker Token Refresh

For long-running clients, provide broker credentials so the SDK automatically refreshes web-tokens on reconnect:

```typescript
const client = new FreeqClient({
  url: 'wss://irc.freeq.at/irc',
  nick: 'persistent-bot',
  sasl: { token, did, pdsUrl, method },
  brokerUrl: 'https://auth.freeq.at',
  brokerToken: 'long-lived-broker-token',
});
```

## Events

The SDK uses a typed event emitter. Every state change is delivered as an event — subscribe to exactly what you need.

### Connection Events

| Event | Payload | Description |
|-------|---------|-------------|
| `connectionStateChanged` | `(state: TransportState)` | `'disconnected'`, `'connecting'`, or `'connected'` |
| `registered` | `(nick: string)` | IRC registration complete (001 received) |
| `ready` | `()` | Fully connected and channels joined |
| `nickChanged` | `(nick: string)` | Our nickname changed |
| `authenticated` | `(did: string, message: string)` | SASL authentication succeeded |
| `authError` | `(error: string)` | SASL authentication failed |
| `error` | `(message: string)` | Server ERROR received |

### Message Events

| Event | Payload | Description |
|-------|---------|-------------|
| `message` | `(channel: string, msg: Message)` | New message in a channel or DM |
| `messageEdited` | `(channel, msgId, newText, newMsgId?, isStreaming?)` | A message was edited |
| `messageDeleted` | `(channel: string, msgId: string)` | A message was deleted |
| `reactionAdded` | `(channel, msgId, emoji, fromNick)` | Reaction added to a message |
| `systemMessage` | `(target: string, text: string)` | Server notice or system event |

### Channel Events

| Event | Payload | Description |
|-------|---------|-------------|
| `channelJoined` | `(channel: string)` | We joined a channel |
| `channelLeft` | `(channel: string)` | We left or were kicked from a channel |
| `topicChanged` | `(channel, topic, setBy?)` | Channel topic changed |
| `modeChanged` | `(channel, mode, arg?, setBy)` | Channel mode changed |
| `historyBatch` | `(channel: string, messages: Message[])` | Chat history batch received |

### Member Events

| Event | Payload | Description |
|-------|---------|-------------|
| `memberJoined` | `(channel, member)` | User joined a channel |
| `memberLeft` | `(channel: string, nick: string)` | User left a channel |
| `membersList` | `(channel, members[])` | NAMES list received |
| `memberDid` | `(nick: string, did: string)` | User's DID discovered via WHOIS |
| `userQuit` | `(nick: string, reason: string)` | User disconnected |
| `userRenamed` | `(oldNick, newNick)` | User changed nick |
| `userAway` | `(nick, reason: string \| null)` | Away status changed |
| `typing` | `(channel, nick, isTyping)` | Typing indicator |
| `userKicked` | `(channel, kicked, by, reason)` | User kicked from channel |

### Other Events

| Event | Payload | Description |
|-------|---------|-------------|
| `whois` | `(nick, info: Partial<WhoisInfo>)` | WHOIS information received |
| `dmTarget` | `(nick: string)` | DM conversation target discovered |
| `pins` | `(channel, pins: PinnedMessage[])` | Pinned messages fetched |
| `pinAdded` / `pinRemoved` | `(channel, msgid, ...)` | Pin changed |
| `channelListEntry` | `(entry: ChannelListEntry)` | Channel from LIST response |
| `invited` | `(channel, by)` | Invited to a channel |
| `joinGateRequired` | `(channel: string)` | Policy acceptance needed to join |
| `motd` | `(line: string)` | MOTD line received |
| `raw` | `(line: string, parsed: IRCMessage)` | Raw IRC line (for debugging) |

### Example: Event Handling

```typescript
// Subscribe
const handler = (channel: string, msg: Message) => {
  console.log(`${msg.from}: ${msg.text}`);
};
client.on('message', handler);

// Unsubscribe
client.off('message', handler);

// One-time listener
client.once('ready', () => {
  console.log('First connection established');
});
```

## Sending Messages

```typescript
// Simple message
client.sendMessage('#general', 'Hello world');

// Multi-line message
client.sendMessage('#general', 'Line 1\nLine 2\nLine 3', true);

// Markdown
client.sendMarkdown('#general', '**bold** and `code`');

// Reply to a message
client.sendReply('#general', originalMsgId, 'Great point!');

// Edit a message
client.sendEdit('#general', msgId, 'Updated text');

// Delete a message
client.sendDelete('#general', msgId);

// React with emoji
client.sendReaction('#general', '👍', msgId);
```

## Channel Management

```typescript
// Join / leave
client.join('#mychannel');
client.part('#mychannel');

// Topic
client.setTopic('#mychannel', 'Welcome to my channel');

// Modes
client.setMode('#mychannel', '+o', 'someuser');  // Op a user
client.setMode('#mychannel', '+i');                // Invite-only

// Moderation
client.kick('#mychannel', 'spammer', 'No spam');
client.invite('#mychannel', 'friend');

// Pin messages
client.pin('#mychannel', msgId);
client.unpin('#mychannel', msgId);
```

## Chat History

The SDK supports IRCv3 CHATHISTORY for fetching older messages:

```typescript
// Fetch latest 50 messages
client.requestHistory('#general');

// Fetch 50 messages before a timestamp
client.requestHistory('#general', '2024-01-15T10:30:00Z');

// Listen for history batches
client.on('historyBatch', (channel, messages) => {
  console.log(`Got ${messages.length} history messages for ${channel}`);
  for (const msg of messages) {
    console.log(`  [${msg.timestamp.toISOString()}] ${msg.from}: ${msg.text}`);
  }
});

// Fetch DM conversation list
client.requestDmTargets();
client.on('dmTarget', (nick) => {
  console.log(`DM conversation with: ${nick}`);
});
```

## End-to-End Encryption

### Channel Encryption (ENC1)

Passphrase-based AES-256-GCM encryption for channels. All members must know the passphrase:

```typescript
// Set a channel passphrase
await client.setChannelEncryption('#secret', 'my-passphrase');

// Messages are now automatically encrypted/decrypted
client.sendMessage('#secret', 'This is encrypted');

// Remove encryption
client.removeChannelEncryption('#secret');
```

### DM Encryption (ENC3)

Automatic Double Ratchet encryption for DMs between AT Protocol users. Enabled automatically after authentication:

```typescript
client.on('authenticated', async (did) => {
  // E2EE initializes automatically after SASL success.
  // DMs with other authenticated users are encrypted transparently.
  console.log('E2EE ready for DMs');
});

// Verify a DM partner's identity
const safetyNumber = await client.getSafetyNumber('did:plc:abc123');
console.log('Safety number:', safetyNumber);
// → "12345 67890 11111 22222 33333 44444 55555 66666 77777 88888 99999 00000"
```

Encrypted messages have `encrypted: true` on the `Message` object.

## AT Protocol Profiles

Fetch Bluesky profiles for any DID or handle:

```typescript
import { fetchProfile, getCachedProfile, prefetchProfiles } from '@freeq/sdk';

// Fetch a profile (cached for 10 minutes)
const profile = await fetchProfile('did:plc:abc123');
console.log(profile?.displayName, profile?.avatar);

// Batch prefetch (non-blocking)
prefetchProfiles(['did:plc:aaa', 'did:plc:bbb', 'did:plc:ccc']);

// Read from cache (synchronous, returns null if not cached)
const cached = getCachedProfile('did:plc:abc123');
```

## IRC Protocol Utilities

The SDK exports low-level IRC utilities for advanced use cases:

```typescript
import { parse, format, prefixNick } from '@freeq/sdk';

// Parse a raw IRC line
const msg = parse('@msgid=abc123 :nick!user@host PRIVMSG #channel :Hello');
// → { tags: { msgid: 'abc123' }, prefix: 'nick!user@host', command: 'PRIVMSG', params: ['#channel', 'Hello'] }

// Extract nick from prefix
prefixNick('nick!user@host'); // → 'nick'

// Format an IRC line
format('PRIVMSG', ['#channel', 'Hello'], { '+reply': 'abc123' });
// → '@+reply=abc123 PRIVMSG #channel :Hello'
```

## Raw Commands

Send any IRC command directly:

```typescript
client.raw('LIST');
client.raw('WHOIS someuser');
client.raw('OPER admin secretpassword');
```

## Client State

Access connection state at any time:

```typescript
client.nick;              // Current nickname
client.authDid;           // Authenticated DID or null
client.connectionState;   // 'disconnected' | 'connecting' | 'connected'
client.registered;        // true after IRC 001
client.joinedChannels;    // Set<string> of channel names (lowercase)
```

## Reconnection

The SDK automatically reconnects with exponential backoff (1s → 2s → 4s → ... → 30s max). You can also force a reconnect:

```typescript
client.reconnect();  // Disconnect and immediately reconnect
```

## Types

All types are exported and fully documented:

```typescript
import type {
  Message,           // Chat message with reactions, encryption status, etc.
  Member,            // Channel member with roles, DID, away status
  Channel,           // Channel with members, messages, modes, pins
  WhoisInfo,         // WHOIS response data
  IRCMessage,        // Parsed IRC protocol message
  TransportState,    // Connection state union
  SaslCredentials,   // AT Protocol auth credentials
  FreeqClientOptions,// Client constructor options
  ATProfile,         // Bluesky profile data
  PinnedMessage,     // Pinned message reference
  ChannelListEntry,  // Channel from LIST response
  AvSession,         // Audio/video session
  AvParticipant,     // AV session participant
  FreeqEvents,       // Event name → handler type map
} from '@freeq/sdk';
```

## Examples

### Echo Bot

```typescript
import { FreeqClient } from '@freeq/sdk';

const client = new FreeqClient({
  url: 'wss://irc.freeq.at/irc',
  nick: 'echobot',
  channels: ['#bots'],
});

client.on('message', (channel, msg) => {
  if (!msg.isSelf && msg.text.startsWith('!echo ')) {
    client.sendMessage(channel, msg.text.slice(6));
  }
});

client.connect();
```

### Logging Bot

```typescript
import { FreeqClient } from '@freeq/sdk';
import { appendFileSync } from 'fs';

const client = new FreeqClient({
  url: 'wss://irc.freeq.at/irc',
  nick: 'logger',
  channels: ['#general', '#dev'],
});

client.on('message', (channel, msg) => {
  if (msg.isSystem) return;
  const line = `[${msg.timestamp.toISOString()}] ${channel} <${msg.from}> ${msg.text}\n`;
  appendFileSync('irc.log', line);
});

client.connect();
```

### Authenticated Bot with E2EE

```typescript
import { FreeqClient } from '@freeq/sdk';

const client = new FreeqClient({
  url: 'wss://irc.freeq.at/irc',
  nick: 'securebot',
  channels: ['#encrypted'],
  sasl: {
    token: process.env.FREEQ_TOKEN!,
    did: process.env.FREEQ_DID!,
    pdsUrl: 'https://bsky.social',
    method: 'pds-session',
  },
});

client.on('authenticated', async () => {
  // Set channel encryption passphrase
  await client.setChannelEncryption('#encrypted', 'shared-secret');
});

client.on('message', (channel, msg) => {
  const lock = msg.encrypted ? '🔒' : '  ';
  console.log(`${lock} [${channel}] ${msg.from}: ${msg.text}`);
});

client.connect();
```

### Monitoring Dashboard

```typescript
import { FreeqClient, fetchProfile } from '@freeq/sdk';

const client = new FreeqClient({
  url: 'wss://irc.freeq.at/irc',
  nick: 'monitor',
  channels: ['#ops'],
});

client.on('memberJoined', async (channel, member) => {
  if (member.did) {
    const profile = await fetchProfile(member.did);
    console.log(`→ ${member.nick} joined ${channel} (${profile?.displayName || 'unknown'})`);
  }
});

client.on('userQuit', (nick, reason) => {
  console.log(`← ${nick} quit: ${reason}`);
});

client.on('topicChanged', (channel, topic, setBy) => {
  console.log(`📋 ${channel} topic: "${topic}" (by ${setBy})`);
});

client.connect();
```

## Package Exports

The SDK provides multiple entry points:

```typescript
// Main SDK (client, types, parser, profiles)
import { FreeqClient, parse, fetchProfile } from '@freeq/sdk';

// E2EE module (for direct access to encryption primitives)
import { isEncrypted, getSafetyNumber } from '@freeq/sdk/e2ee';

// Profiles module (standalone)
import { fetchProfile } from '@freeq/sdk/profiles';
```

## Source

The SDK source is at [`freeq-sdk-js/`](https://github.com/chad/freeq/tree/main/freeq-sdk-js) in the freeq repository.

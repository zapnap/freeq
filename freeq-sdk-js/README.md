# @freeq/sdk

TypeScript SDK for building [freeq](https://freeq.at) IRC clients, bots, and integrations.

Handles IRC protocol, IRCv3 capabilities, AT Protocol (Bluesky) authentication, and end-to-end encryption — so you can focus on your application logic.

**Framework-agnostic.** No React, no DOM dependencies. Works in browsers, Node.js, Deno, and Bun.

## Install

```bash
npm install @freeq/sdk
```

## Quick Start

```typescript
import { FreeqClient } from '@freeq/sdk';

const client = new FreeqClient({
  url: 'wss://irc.freeq.at/irc',
  nick: 'mybot',
  channels: ['#general'],
});

client.on('message', (channel, msg) => {
  console.log(`[${channel}] ${msg.from}: ${msg.text}`);
});

client.on('ready', () => {
  client.sendMessage('#general', 'Hello from the SDK!');
});

client.connect();
```

## Documentation

Full documentation with examples: **[freeq.at/docs/typescript-sdk/](https://freeq.at/docs/typescript-sdk/)**

## Features

- **Event-driven** — typed events for messages, members, channels, modes, and more
- **AT Protocol auth** — SASL ATPROTO-CHALLENGE with broker token refresh
- **E2EE** — Double Ratchet for DMs, AES-256-GCM for channels
- **IRCv3** — message-tags, echo-message, CHATHISTORY, batch, away-notify
- **Auto-reconnect** — exponential backoff with automatic channel rejoin
- **Message signing** — Ed25519 per-session signing for non-repudiation
- **Profiles** — AT Protocol profile fetching with TTL cache
- **Zero framework deps** — pure TypeScript, no React/Angular/Vue required

## License

MIT

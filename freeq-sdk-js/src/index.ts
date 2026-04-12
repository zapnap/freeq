/**
 * @freeq/sdk — TypeScript SDK for building freeq IRC clients.
 *
 * @example
 * ```typescript
 * import { FreeqClient } from '@freeq/sdk';
 *
 * const client = new FreeqClient({
 *   url: 'wss://irc.freeq.at/irc',
 *   nick: 'mybot',
 * });
 *
 * client.on('message', (channel, msg) => {
 *   console.log(`[${channel}] ${msg.from}: ${msg.text}`);
 * });
 *
 * client.on('ready', () => {
 *   client.join('#mychannel');
 *   client.sendMessage('#mychannel', 'Hello from the SDK!');
 * });
 *
 * client.connect();
 * ```
 */

// Main client
export { FreeqClient } from './client.js';

// Event types
export type { FreeqEvents } from './events.js';

// IRC protocol utilities
export { parse, format, prefixNick } from './parser.js';

// Transport
export { Transport } from './transport.js';

// Types
export type {
  IRCMessage,
  Message,
  Member,
  Channel,
  PinnedMessage,
  WhoisInfo,
  ChannelListEntry,
  AvSession,
  AvParticipant,
  TransportState,
  SaslCredentials,
  FreeqClientOptions,
  Batch,
} from './types.js';

// Profiles
export { fetchProfile, prefetchProfiles, getCachedProfile } from './profiles.js';
export type { ATProfile } from './profiles.js';

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
// IRC protocol utilities
export { parse, format, prefixNick } from './parser.js';
// Transport
export { Transport } from './transport.js';
// Profiles
export { fetchProfile, prefetchProfiles, getCachedProfile } from './profiles.js';
//# sourceMappingURL=index.js.map
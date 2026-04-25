/**
 * FreeqClient — event-driven IRC client with AT Protocol identity and E2EE.
 *
 * Usage:
 *   const client = new FreeqClient({ url: 'wss://irc.freeq.at/irc', nick: 'mybot' });
 *   client.on('message', (channel, msg) => console.log(`${msg.from}: ${msg.text}`));
 *   client.connect();
 */
import { EventEmitter } from './events.js';
import type { AvSession, FreeqClientOptions, SaslCredentials, TransportState } from './types.js';
export declare class FreeqClient extends EventEmitter {
    private transport;
    private _nick;
    private _authDid;
    private _connectionState;
    private _registered;
    private opts;
    private ackedCaps;
    private sasl;
    private skipBrokerRefresh;
    private guestFallbackCount;
    /** Set when SASL was attempted and 904 was received. Suppresses any
     *  subsequent registration completion as a guest, and blocks outgoing
     *  PRIVMSGs that would silently leak under the guest identity. */
    private _saslFailed;
    private autoJoinChannels;
    private _joinedChannels;
    private backgroundWhois;
    private echoPlaintextCache;
    private batches;
    private pendingAwayReason;
    private _avSessions;
    private _activeAvSession;
    constructor(opts: FreeqClientOptions);
    /** Current IRC nickname. */
    get nick(): string;
    /** Authenticated AT Protocol DID, or null if guest. */
    get authDid(): string | null;
    /** Current connection state. */
    get connectionState(): TransportState;
    /** Whether IRC registration is complete (001 received). */
    get registered(): boolean;
    /** Set of channels we're currently in (lowercase). */
    get joinedChannels(): ReadonlySet<string>;
    /** Active AV sessions. */
    get avSessions(): ReadonlyMap<string, AvSession>;
    /** Active AV session ID we're participating in. */
    get activeAvSession(): string | null;
    /** Server origin for API calls. */
    get serverOrigin(): string;
    /** Connect to the IRC server. */
    connect(): void;
    /** Disconnect from the server. */
    disconnect(): void;
    /** Force an immediate reconnect. */
    reconnect(): void;
    /** Set SASL credentials (call before connect, or before reconnect). */
    setSaslCredentials(creds: SaslCredentials): void;
    /** Send a message to a channel or user. */
    sendMessage(target: string, text: string, multiline?: boolean): void;
    /** Send a reply to a specific message. */
    sendReply(target: string, replyToMsgId: string, text: string, multiline?: boolean): void;
    /** Edit a previously sent message. */
    sendEdit(target: string, originalMsgId: string, newText: string, multiline?: boolean): void;
    /** Send a message with Markdown formatting. */
    sendMarkdown(target: string, text: string): void;
    /** Delete a message. */
    sendDelete(target: string, msgId: string): void;
    /** React to a message with an emoji. */
    sendReaction(target: string, emoji: string, msgId?: string): void;
    /** Join a channel. */
    join(channel: string): void;
    /** Leave a channel. */
    part(channel: string): void;
    /** Set a channel's topic. */
    setTopic(channel: string, topic: string): void;
    /** Set a channel or user mode. */
    setMode(channel: string, mode: string, arg?: string): void;
    /** Kick a user from a channel. */
    kick(channel: string, nick: string, reason?: string): void;
    /** Invite a user to a channel. */
    invite(channel: string, nick: string): void;
    /** Set or clear away status. */
    setAway(reason?: string): void;
    /** Send a WHOIS query. */
    whois(nick: string): void;
    /** Request chat history for a channel. */
    requestHistory(channel: string, before?: string): void;
    /** Request DM conversation targets. */
    requestDmTargets(limit?: number): void;
    /** Pin a message. */
    pin(channel: string, msgid: string): void;
    /** Unpin a message. */
    unpin(channel: string, msgid: string): void;
    /** Send a raw IRC command. */
    raw(line: string): void;
    /** Set a channel encryption passphrase (ENC1). */
    setChannelEncryption(channel: string, passphrase: string): Promise<void>;
    /** Remove channel encryption. */
    removeChannelEncryption(channel: string): void;
    /** Initialize E2EE for DMs (called automatically after SASL success). */
    initializeE2EE(did: string): Promise<void>;
    /** Get the E2EE safety number for a DM partner. */
    getSafetyNumber(remoteDid: string): Promise<string | null>;
    /** Fetch pinned messages for a channel via REST API. */
    fetchPins(channel: string): Promise<void>;
    private onTransportStateChange;
    private didForNick;
    /** Resolve nick to DID — set by the app layer for E2EE support. */
    nickToDid: ((nick: string) => string | undefined) | null;
    private resolveNickToDid;
    private signedPrivmsg;
    private cacheEchoPlaintext;
    private handleLine;
    private handleCap;
    private handleAuthenticate;
    private handleAvSessionState;
}
//# sourceMappingURL=client.d.ts.map
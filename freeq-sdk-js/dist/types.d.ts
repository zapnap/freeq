/** Core types for the freeq SDK. */
/** Parsed IRC message with optional IRCv3 tags. */
export interface IRCMessage {
    tags: Record<string, string>;
    prefix: string;
    command: string;
    params: string[];
}
/** A chat message. */
export interface Message {
    id: string;
    from: string;
    text: string;
    timestamp: Date;
    tags: Record<string, string>;
    isAction?: boolean;
    isSelf?: boolean;
    isSystem?: boolean;
    replyTo?: string;
    editOf?: string;
    isStreaming?: boolean;
    deleted?: boolean;
    reactions?: Map<string, Set<string>>;
    encrypted?: boolean;
}
/** A channel or DM member. */
export interface Member {
    nick: string;
    did?: string;
    handle?: string;
    displayName?: string;
    avatarUrl?: string;
    isOp: boolean;
    isHalfop: boolean;
    isVoiced: boolean;
    away?: string | null;
    typing?: boolean;
    actorClass?: 'human' | 'agent' | 'external_agent';
}
/** A pinned message reference. */
export interface PinnedMessage {
    msgid: string;
    pinned_by: string;
    pinned_at: number;
}
/** A channel with members and messages. */
export interface Channel {
    name: string;
    topic: string;
    topicSetBy?: string;
    members: Map<string, Member>;
    messages: Message[];
    modes: Set<string>;
    isEncrypted: boolean;
    unreadCount: number;
    mentionCount: number;
    lastReadMsgId?: string;
    isJoined: boolean;
    pins: PinnedMessage[];
}
/** WHOIS information for a user. */
export interface WhoisInfo {
    nick: string;
    user?: string;
    host?: string;
    realname?: string;
    server?: string;
    did?: string;
    handle?: string;
    channels?: string;
    fetchedAt: number;
}
/** An entry in the server's channel list. */
export interface ChannelListEntry {
    name: string;
    topic: string;
    count: number;
}
/** AV session participant. */
export interface AvParticipant {
    did: string;
    nick: string;
    role: 'host' | 'speaker' | 'listener';
    joinedAt: Date;
}
/** AV (audio/video) session. */
export interface AvSession {
    id: string;
    channel: string | null;
    createdBy: string;
    createdByNick: string;
    title?: string;
    participants: Map<string, AvParticipant>;
    state: 'active' | 'ended';
    startedAt: Date;
    irohTicket?: string;
}
/** WebSocket transport state. */
export type TransportState = 'disconnected' | 'connecting' | 'connected';
/** SASL credentials for AT Protocol authentication. */
export interface SaslCredentials {
    token: string;
    did: string;
    pdsUrl: string;
    method: string;
}
/** Options for creating a FreeqClient. */
export interface FreeqClientOptions {
    /** WebSocket URL (e.g. "wss://irc.freeq.at/irc"). */
    url: string;
    /** Desired IRC nickname. */
    nick: string;
    /** Channels to auto-join on connect. */
    channels?: string[];
    /** SASL credentials for AT Protocol authentication. */
    sasl?: SaslCredentials;
    /**
     * Base URL for the auth broker (for session refresh).
     * If set along with `brokerToken`, the client refreshes
     * the web-token on each reconnect.
     */
    brokerUrl?: string;
    /** Long-lived broker token for session refresh. */
    brokerToken?: string;
    /** Server origin for API calls (e.g. E2EE key upload). Defaults to url origin. */
    serverOrigin?: string;
    /** Skip the first broker token refresh (use when token is already fresh). */
    skipInitialBrokerRefresh?: boolean;
}
/** A batch of messages (e.g. CHATHISTORY response). */
export interface Batch {
    type: string;
    target: string;
    messages: Message[];
}
//# sourceMappingURL=types.d.ts.map
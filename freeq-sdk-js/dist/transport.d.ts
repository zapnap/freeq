/** WebSocket IRC transport with auto-reconnect and heartbeat. */
import type { TransportState } from './types.js';
export interface TransportOptions {
    url: string;
    onLine: (line: string) => void;
    onStateChange: (state: TransportState) => void;
}
export declare class Transport {
    private ws;
    private opts;
    private reconnectTimer;
    private reconnectAttempts;
    private intentionalClose;
    private lastDataReceived;
    private heartbeatTimer;
    private static PING_INTERVAL;
    private static DEAD_TIMEOUT;
    constructor(opts: TransportOptions);
    connect(): void;
    send(line: string): void;
    disconnect(): void;
    private startHeartbeat;
    private stopHeartbeat;
    private scheduleReconnect;
}
//# sourceMappingURL=transport.d.ts.map
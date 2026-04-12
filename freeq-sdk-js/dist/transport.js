/** WebSocket IRC transport with auto-reconnect and heartbeat. */
export class Transport {
    ws = null;
    opts;
    reconnectTimer = null;
    reconnectAttempts = 0;
    intentionalClose = false;
    lastDataReceived = 0;
    heartbeatTimer = null;
    static PING_INTERVAL = 45_000;
    static DEAD_TIMEOUT = 90_000;
    constructor(opts) {
        this.opts = opts;
    }
    connect() {
        this.intentionalClose = false;
        this.opts.onStateChange('connecting');
        try {
            this.ws = new WebSocket(this.opts.url);
        }
        catch {
            this.opts.onStateChange('disconnected');
            this.scheduleReconnect();
            return;
        }
        this.ws.onopen = () => {
            this.reconnectAttempts = 0;
            this.lastDataReceived = Date.now();
            this.opts.onStateChange('connected');
            this.startHeartbeat();
        };
        this.ws.onmessage = (e) => {
            this.lastDataReceived = Date.now();
            const data = typeof e.data === 'string' ? e.data : '';
            for (const line of data.split('\n')) {
                const trimmed = line.replace(/\r$/, '');
                if (trimmed)
                    this.opts.onLine(trimmed);
            }
        };
        this.ws.onclose = () => {
            this.stopHeartbeat();
            this.opts.onStateChange('disconnected');
            if (!this.intentionalClose) {
                this.scheduleReconnect();
            }
        };
        this.ws.onerror = () => {
            // onclose will fire after this
        };
    }
    send(line) {
        if (this.ws?.readyState === WebSocket.OPEN) {
            if (this.ws.bufferedAmount > 65536) {
                console.warn('[transport] High bufferedAmount, forcing reconnect');
                this.ws.close();
                return;
            }
            this.ws.send(line);
        }
        else {
            console.warn('[transport] Dropped message (ws not open):', line);
        }
    }
    disconnect() {
        this.intentionalClose = true;
        this.stopHeartbeat();
        if (this.reconnectTimer) {
            clearTimeout(this.reconnectTimer);
            this.reconnectTimer = null;
        }
        if (this.ws) {
            try {
                this.send('QUIT :Leaving');
            }
            catch { /* ignore */ }
            this.ws.close();
            this.ws = null;
        }
        this.opts.onStateChange('disconnected');
    }
    startHeartbeat() {
        this.stopHeartbeat();
        this.heartbeatTimer = setInterval(() => {
            const elapsed = Date.now() - this.lastDataReceived;
            if (elapsed > Transport.DEAD_TIMEOUT) {
                console.warn('[transport] No data for 90s, forcing reconnect');
                this.stopHeartbeat();
                if (this.ws) {
                    this.ws.close();
                    this.ws = null;
                }
            }
            else if (elapsed > Transport.PING_INTERVAL) {
                this.send('PING :heartbeat');
            }
        }, 15_000);
    }
    stopHeartbeat() {
        if (this.heartbeatTimer) {
            clearInterval(this.heartbeatTimer);
            this.heartbeatTimer = null;
        }
    }
    scheduleReconnect() {
        if (this.reconnectTimer || this.intentionalClose)
            return;
        this.reconnectAttempts++;
        const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts - 1), 30000);
        this.reconnectTimer = setTimeout(() => {
            this.reconnectTimer = null;
            this.connect();
        }, delay);
    }
}
//# sourceMappingURL=transport.js.map
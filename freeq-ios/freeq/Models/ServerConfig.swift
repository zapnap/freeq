import Foundation

/// Central server configuration - change here to point to a different server
struct ServerConfig {
    /// IRC server host:port — used as the TCP fallback when WebSocket can't
    /// be reached. Many cellular / captive-portal / corporate networks block
    /// raw 6667, which is why we prefer the WebSocket URL below.
    static var ircServer: String = "irc.freeq.at:6667"

    /// Primary IRC transport — same WebSocket endpoint the web client uses.
    /// Lives on port 443, so it traverses every firewall that allows HTTPS.
    static var wssServer: String = "wss://irc.freeq.at/irc"

    /// HTTPS API base URL (derived from ircServer)
    static var apiBaseUrl: String {
        let host = ircServer.components(separatedBy: ":").first ?? ircServer
        return "https://\(host)"
    }
}

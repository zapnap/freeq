import XCTest
@testable import freeq

/// Buffer-routing invariants: anything in `appState.channels` must look like
/// an IRC channel (`#` or `&` prefix); anything in `appState.dmBuffers` must
/// look like a peer nick (no channel prefix).
///
/// Real bug observed: a DM with an agent (`@yokota`) appeared in the Channels
/// pane. That can only happen if a non-channel name landed in `channels` —
/// either via a direct call to `getOrCreateChannel`, or via an event handler
/// that received a non-prefixed target from the wire (e.g. an agent path on
/// the server). The tests below exercise both directions.
final class BufferRoutingTests: XCTestCase {

    private func makeState() -> AppState {
        // AppState() reads UserDefaults / Keychain in init. For tests we want
        // a clean slate per test, so blow those away first.
        for k in ["freeq.nick", "freeq.server", "freeq.channels", "freeq.readPositions",
                  "freeq.unreadCounts", "freeq.mutedChannels"] {
            UserDefaults.standard.removeObject(forKey: k)
        }
        return AppState()
    }

    // MARK: - getOrCreateChannel must reject names that aren't channels

    func testGetOrCreateChannelRejectsBareNick() {
        let s = makeState()
        // Pre-condition.
        XCTAssertTrue(s.channels.isEmpty)
        XCTAssertTrue(s.dmBuffers.isEmpty)

        // A bare nick (no `#` / `&` prefix) is NOT a channel. Even if some
        // future code path mistakenly hands it to getOrCreateChannel, we must
        // not pollute `channels` — otherwise the Channels pane shows a DM peer.
        _ = s.getOrCreateChannel("yokota")

        XCTAssertFalse(
            s.channels.contains(where: { $0.name == "yokota" }),
            "getOrCreateChannel must not append a non-channel-prefixed name to `channels` — `yokota` is a peer nick, not a channel"
        )
    }

    func testGetOrCreateChannelAcceptsHashAndAmpPrefixes() {
        let s = makeState()
        _ = s.getOrCreateChannel("#freeq")
        _ = s.getOrCreateChannel("&local")
        XCTAssertTrue(s.channels.contains(where: { $0.name == "#freeq" }))
        XCTAssertTrue(s.channels.contains(where: { $0.name == "&local" }))
    }

    // MARK: - getOrCreateDM must reject channel-prefixed names

    func testGetOrCreateDMRejectsChannelPrefix() {
        let s = makeState()
        _ = s.getOrCreateDM("#freeq")
        XCTAssertFalse(
            s.dmBuffers.contains(where: { $0.name == "#freeq" }),
            "getOrCreateDM must not accept a channel-prefixed name into `dmBuffers`"
        )
    }

    // MARK: - Cross-list invariant

    func testNoChannelEverContainsBareNickAndNoDMEverContainsChannelPrefix() {
        let s = makeState()
        // Mix of inputs that the wire could plausibly hand us, including the
        // adversarial bare-nick case that produced the @yokota-in-channels bug.
        let inputs = ["#room", "&local", "yokota", "alice", "#another"]
        for name in inputs {
            _ = s.getOrCreateChannel(name)
            _ = s.getOrCreateDM(name)
        }

        for ch in s.channels {
            XCTAssertTrue(
                ch.name.hasPrefix("#") || ch.name.hasPrefix("&"),
                "every entry in `channels` must be a real channel name; got `\(ch.name)`"
            )
        }
        for dm in s.dmBuffers {
            XCTAssertFalse(
                dm.name.hasPrefix("#") || dm.name.hasPrefix("&"),
                "no entry in `dmBuffers` may have a channel prefix; got `\(dm.name)`"
            )
        }
    }
}

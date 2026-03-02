import SwiftUI

struct SidebarView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        @Bindable var state = appState
        List(selection: $state.activeChannel) {
            // Channels
            Section("Channels") {
                ForEach(appState.channels) { channel in
                    ChannelRow(channel: channel)
                        .tag(channel.name)
                }
            }

            // DMs
            if !appState.dmBuffers.isEmpty {
                Section("Direct Messages") {
                    ForEach(appState.dmBuffers.sorted(by: { $0.lastActivity > $1.lastActivity })) { dm in
                        DMRow(dm: dm)
                            .tag(dm.name)
                    }
                }
            }

            // P2P connections
            if !appState.p2pConnectedPeers.isEmpty {
                Section("P2P Direct") {
                    ForEach(Array(appState.p2pConnectedPeers), id: \.self) { peerId in
                        Label {
                            Text(String(peerId.prefix(12)) + "…")
                                .font(.system(.body, design: .monospaced))
                        } icon: {
                            Image(systemName: "point.3.connected.trianglepath.dotted")
                                .foregroundStyle(.green)
                        }
                        .tag("p2p:\(String(peerId.prefix(8)))")
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 0) {
                Divider()
                bottomBar
            }
        }
        .onChange(of: appState.activeChannel) { _, newValue in
            if let ch = newValue {
                appState.clearUnread(ch)
            }
        }
    }

    @ViewBuilder
    private var bottomBar: some View {
        HStack(spacing: 8) {
            // User info
            if let did = appState.authenticatedDID {
                Circle()
                    .fill(.green)
                    .frame(width: 8, height: 8)
                Text(appState.nick)
                    .font(.caption)
                    .lineLimit(1)
            } else {
                Circle()
                    .fill(.gray)
                    .frame(width: 8, height: 8)
                Text(appState.nick.isEmpty ? "Not connected" : "\(appState.nick) (guest)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()

            // P2P status
            if appState.isP2pActive {
                Image(systemName: "point.3.connected.trianglepath.dotted")
                    .font(.caption)
                    .foregroundStyle(.green)
                    .help("iroh P2P: \(appState.p2pConnectedPeers.count) peers")
            }

            Button {
                appState.showJoinSheet = true
            } label: {
                Image(systemName: "plus.bubble")
            }
            .buttonStyle(.plain)
            .help("Join Channel")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
    }
}

struct ChannelRow: View {
    @Environment(AppState.self) private var appState
    let channel: ChannelState

    private var unread: Int {
        appState.unreadCounts[channel.name.lowercased()] ?? 0
    }

    var body: some View {
        Label {
            HStack {
                Text(channel.name.replacingOccurrences(of: "#", with: ""))
                    .lineLimit(1)
                Spacer()
                if unread > 0 {
                    Text("\(unread)")
                        .font(.caption2.weight(.bold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Capsule().fill(.red))
                }
            }
        } icon: {
            Image(systemName: "number")
                .foregroundStyle(.secondary)
        }
    }
}

struct DMRow: View {
    @Environment(AppState.self) private var appState
    let dm: ChannelState

    private var isOnline: Bool {
        appState.isNickOnline(dm.name)
    }

    private var unread: Int {
        appState.unreadCounts[dm.name.lowercased()] ?? 0
    }

    var body: some View {
        Label {
            HStack {
                Text(dm.name)
                    .lineLimit(1)

                // P2P indicator
                if appState.p2pDMActive.contains(dm.name.lowercased()) {
                    Image(systemName: "point.3.connected.trianglepath.dotted")
                        .font(.caption2)
                        .foregroundStyle(.green)
                        .help("Direct P2P connection via iroh")
                }

                Spacer()
                if unread > 0 {
                    Text("\(unread)")
                        .font(.caption2.weight(.bold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Capsule().fill(.red))
                }
            }
        } icon: {
            Circle()
                .fill(isOnline ? .green : Color.secondary.opacity(0.3))
                .frame(width: 10, height: 10)
        }
    }
}

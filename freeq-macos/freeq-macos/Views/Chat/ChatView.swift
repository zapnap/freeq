import SwiftUI

/// Main chat area: TopBar + MessageList + ComposeBar
struct ChatView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        VStack(spacing: 0) {
            TopBarView()
            Divider()
            MessageListView()
            Divider()
            ComposeBar()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct TopBarView: View {
    @Environment(AppState.self) private var appState

    private var channel: ChannelState? { appState.activeChannelState }
    private var isChannel: Bool { channel?.isChannel ?? false }

    var body: some View {
        HStack(spacing: 10) {
            // Channel/DM name
            if isChannel {
                Image(systemName: "number")
                    .foregroundStyle(.secondary)
                Text(channel?.name.replacingOccurrences(of: "#", with: "") ?? "")
                    .font(.headline)
            } else {
                Image(systemName: "person.fill")
                    .foregroundStyle(.secondary)
                Text(channel?.name ?? "")
                    .font(.headline)

                // P2P badge
                if let name = channel?.name,
                   appState.p2pDMActive.contains(name.lowercased()) {
                    Label("Direct", systemImage: "point.3.connected.trianglepath.dotted")
                        .font(.caption2)
                        .foregroundStyle(.green)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Capsule().fill(.green.opacity(0.1)))
                }
            }

            if isChannel, let topic = channel?.topic, !topic.isEmpty {
                Divider().frame(height: 16)
                Text(topic)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            // Typing indicators
            if let typers = channel?.activeTypers, !typers.isEmpty {
                Text(typingText(typers))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .italic()
            }

            // Member count (channels only)
            if isChannel {
                Label("\(channel?.members.count ?? 0)", systemImage: "person.2")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            // Detail panel toggle
            Button {
                appState.showDetailPanel.toggle()
            } label: {
                Image(systemName: appState.showDetailPanel ? "sidebar.trailing" : "sidebar.trailing")
                    .foregroundStyle(appState.showDetailPanel ? .primary : .secondary)
            }
            .buttonStyle(.plain)
            .help("Toggle detail panel")
        }
        .padding(.horizontal, 16)
        .frame(height: 44)
        .background(.bar)
    }

    private func typingText(_ typers: [String]) -> String {
        switch typers.count {
        case 1: return "\(typers[0]) is typing…"
        case 2: return "\(typers[0]) and \(typers[1]) are typing…"
        default: return "Several people are typing…"
        }
    }
}

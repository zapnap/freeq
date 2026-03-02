import SwiftUI

/// Right panel — Member list for channels, Profile for DMs, P2P info
struct DetailPanel: View {
    @Environment(AppState.self) private var appState

    private var channel: ChannelState? { appState.activeChannelState }

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            if let ch = channel {
                if ch.isChannel {
                    MemberListView(channel: ch)
                } else {
                    DMProfilePanel(nick: ch.name)
                }
            }
        }
        .background(.bar)
    }
}

struct MemberListView: View {
    let channel: ChannelState

    private var ops: [MemberInfo] { channel.members.filter(\.isOp).sorted { $0.nick < $1.nick } }
    private var voiced: [MemberInfo] { channel.members.filter { !$0.isOp && $0.isVoiced }.sorted { $0.nick < $1.nick } }
    private var regular: [MemberInfo] { channel.members.filter { !$0.isOp && !$0.isVoiced }.sorted { $0.nick < $1.nick } }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if !ops.isEmpty {
                    memberSection("Operators — \(ops.count)", members: ops)
                }
                if !voiced.isEmpty {
                    memberSection("Voiced — \(voiced.count)", members: voiced)
                }
                memberSection(
                    "\(ops.isEmpty && voiced.isEmpty ? "Online" : "Members") — \(regular.count)",
                    members: regular
                )
            }
            .padding(.vertical, 8)
        }
    }

    @ViewBuilder
    func memberSection(_ title: String, members: [MemberInfo]) -> some View {
        Text(title)
            .font(.caption.weight(.bold))
            .foregroundStyle(.tertiary)
            .textCase(.uppercase)
            .padding(.horizontal, 12)
            .padding(.top, 12)
            .padding(.bottom, 4)

        ForEach(members) { member in
            MemberRow(member: member)
        }
    }
}

struct MemberRow: View {
    let member: MemberInfo

    var body: some View {
        HStack(spacing: 8) {
            // Avatar placeholder
            ZStack {
                Circle()
                    .fill(Theme.nickColor(for: member.nick).opacity(0.2))
                    .frame(width: 28, height: 28)
                Text(String(member.nick.prefix(1)).uppercased())
                    .font(.caption.weight(.bold))
                    .foregroundStyle(Theme.nickColor(for: member.nick))
            }
            .overlay(alignment: .bottomTrailing) {
                Circle()
                    .fill(member.isAway ? .orange : .green)
                    .frame(width: 8, height: 8)
                    .overlay(
                        Circle().strokeBorder(Color(nsColor: .windowBackgroundColor), lineWidth: 1.5)
                    )
            }

            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 3) {
                    if !member.prefix.isEmpty {
                        Text(member.prefix)
                            .font(.caption.weight(.bold))
                            .foregroundStyle(member.isOp ? .green : .orange)
                    }
                    Text(member.nick)
                        .font(.system(.body, weight: member.isAway ? .regular : .medium))
                        .foregroundStyle(member.isAway ? .secondary : .primary)
                        .lineLimit(1)

                    if member.isVerified {
                        Image(systemName: "checkmark.seal.fill")
                            .font(.caption2)
                            .foregroundStyle(.blue)
                    }
                }
            }
            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .contentShape(Rectangle())
    }
}

struct DMProfilePanel: View {
    @Environment(AppState.self) private var appState
    let nick: String

    private var isOnline: Bool { appState.isNickOnline(nick) }
    private var awayMsg: String? { appState.awayStatus(for: nick) }
    private var isP2p: Bool { appState.p2pDMActive.contains(nick.lowercased()) }

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                // Banner
                LinearGradient(
                    colors: [Theme.nickColor(for: nick).opacity(0.3), .clear],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
                .frame(height: 80)
                .overlay(alignment: .bottom) {
                    ZStack {
                        Circle()
                            .fill(Theme.nickColor(for: nick).opacity(0.2))
                            .frame(width: 56, height: 56)
                        Text(String(nick.prefix(1)).uppercased())
                            .font(.title.weight(.bold))
                            .foregroundStyle(Theme.nickColor(for: nick))
                    }
                    .overlay(alignment: .bottomTrailing) {
                        Circle()
                            .fill(isOnline ? (awayMsg != nil ? .orange : .green) : Color.secondary.opacity(0.3))
                            .frame(width: 14, height: 14)
                            .overlay(Circle().strokeBorder(.background, lineWidth: 2))
                    }
                    .offset(y: 28)
                }

                VStack(spacing: 6) {
                    Text(nick)
                        .font(.headline)
                        .padding(.top, 32)

                    // Status
                    if isOnline {
                        if let away = awayMsg {
                            Label("Away: \(away)", systemImage: "moon.fill")
                                .font(.caption)
                                .foregroundStyle(.orange)
                        } else {
                            Label("Online", systemImage: "circle.fill")
                                .font(.caption)
                                .foregroundStyle(.green)
                        }
                    } else {
                        Label("Offline", systemImage: "circle")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }

                    // P2P connection info
                    if isP2p {
                        Label("Direct P2P via iroh", systemImage: "point.3.connected.trianglepath.dotted")
                            .font(.caption)
                            .foregroundStyle(.green)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 4)
                            .background(Capsule().fill(.green.opacity(0.1)))
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
            }
        }
    }
}

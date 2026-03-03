import SwiftUI

/// Right panel — Member list for channels, Profile for DMs, P2P info
struct DetailPanel: View {
    @Environment(AppState.self) private var appState

    private var channel: ChannelState? { appState.activeChannelState }

    var body: some View {
        VStack(spacing: 0) {
            if let ch = channel {
                if ch.isChannel {
                    MemberListView(channel: ch)
                } else {
                    DMProfilePanel(nick: ch.name)
                }
            }
        }
        .background(Color(nsColor: .controlBackgroundColor))
    }
}

struct MemberListView: View {
    @Environment(AppState.self) private var appState
    let channel: ChannelState
    @State private var searchText: String = ""

    private var ops: [MemberInfo] { filtered.filter(\.isOp).sorted { $0.nick < $1.nick } }
    private var voiced: [MemberInfo] { filtered.filter { !$0.isOp && $0.isVoiced }.sorted { $0.nick < $1.nick } }
    private var regular: [MemberInfo] { filtered.filter { !$0.isOp && !$0.isVoiced }.sorted { $0.nick < $1.nick } }

    private var filtered: [MemberInfo] {
        if searchText.isEmpty { return channel.members }
        let q = searchText.lowercased()
        return channel.members.filter { $0.nick.lowercased().contains(q) }
    }

    var body: some View {
        VStack(spacing: 0) {
            // Search
            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                TextField("Search members", text: $searchText)
                    .textFieldStyle(.plain)
                    .font(.caption)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(Color(nsColor: .textBackgroundColor).opacity(0.5))

            Divider()

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
            MemberRow(member: member, channelName: channel.name)
        }
    }
}

struct MemberRow: View {
    @Environment(AppState.self) private var appState
    let member: MemberInfo
    let channelName: String

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
                }
                if member.isAway, let away = member.awayMsg {
                    Text(away)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
            }
            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .contentShape(Rectangle())
        .onTapGesture {
            // Open DM with this user
            if member.nick.lowercased() != appState.nick.lowercased() {
                let dm = appState.getOrCreateDM(member.nick)
                appState.activeChannel = dm.name
            }
        }
        .contextMenu {
            Button("Send Message") {
                let dm = appState.getOrCreateDM(member.nick)
                appState.activeChannel = dm.name
            }
            Button("WHOIS") {
                appState.sendWhois(member.nick)
            }
            Divider()
            Button("Op") { appState.setMode(channelName, "+o", member.nick) }
            Button("Deop") { appState.setMode(channelName, "-o", member.nick) }
            Button("Voice") { appState.setMode(channelName, "+v", member.nick) }
            Divider()
            Button("Kick", role: .destructive) {
                appState.kickUser(channelName, member.nick)
            }
        }
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
                        Label("Offline — messages saved", systemImage: "circle")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }

                    // P2P
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

                Divider()

                // Actions
                VStack(spacing: 8) {
                    Button {
                        appState.sendWhois(nick)
                    } label: {
                        Label("WHOIS", systemImage: "person.text.rectangle")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                }
                .padding(16)
            }
        }
    }
}

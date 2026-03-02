import SwiftUI

struct ComposeBar: View {
    @Environment(AppState.self) private var appState
    @State private var text: String = ""
    @FocusState private var isFocused: Bool

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            // Text editor (multi-line)
            ZStack(alignment: .topLeading) {
                if text.isEmpty {
                    Text("Message \(appState.activeChannel ?? "")…")
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 8)
                }
                TextEditor(text: $text)
                    .font(.body)
                    .scrollContentBackground(.hidden)
                    .padding(.horizontal, 2)
                    .frame(minHeight: 32, maxHeight: 120)
                    .fixedSize(horizontal: false, vertical: true)
                    .focused($isFocused)
            }
            .padding(4)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(Color(nsColor: .controlBackgroundColor))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(Color(nsColor: .separatorColor), lineWidth: 0.5)
            )

            // Send button
            Button {
                send()
            } label: {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.title2)
                    .symbolRenderingMode(.hierarchical)
                    .foregroundColor(text.isEmpty ? .gray : .accentColor)
            }
            .buttonStyle(.plain)
            .disabled(text.isEmpty)
            .keyboardShortcut(.return, modifiers: [])
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(.bar)
        .onAppear {
            isFocused = true
        }
    }

    private func send() {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let target = appState.activeChannel else { return }

        // Handle slash commands
        if trimmed.hasPrefix("/") {
            handleCommand(trimmed, target: target)
        } else {
            // Split multi-line messages
            for line in trimmed.components(separatedBy: .newlines) {
                let l = line.trimmingCharacters(in: .whitespaces)
                if !l.isEmpty {
                    appState.sendMessage(to: target, text: l)
                }
            }
        }

        text = ""
    }

    private func handleCommand(_ input: String, target: String) {
        let parts = input.dropFirst().split(separator: " ", maxSplits: 1)
        let cmd = parts.first.map(String.init)?.lowercased() ?? ""
        let arg = parts.count > 1 ? String(parts[1]) : ""

        switch cmd {
        case "join":
            appState.joinChannel(arg.isEmpty ? target : arg)
        case "part":
            appState.partChannel(arg.isEmpty ? target : arg)
        case "nick":
            if !arg.isEmpty {
                appState.sendRaw("NICK \(arg)")
            }
        case "me":
            appState.sendRaw("PRIVMSG \(target) :\u{01}ACTION \(arg)\u{01}")
        case "topic":
            appState.sendRaw("TOPIC \(target) :\(arg)")
        case "msg":
            let msgParts = arg.split(separator: " ", maxSplits: 1)
            if msgParts.count == 2 {
                appState.sendMessage(to: String(msgParts[0]), text: String(msgParts[1]))
            }
        case "p2p":
            handleP2pCommand(arg)
        case "raw":
            appState.sendRaw(arg)
        default:
            appState.sendRaw("\(cmd.uppercased()) \(arg)")
        }
    }

    private func handleP2pCommand(_ arg: String) {
        let parts = arg.split(separator: " ", maxSplits: 1)
        let subcmd = parts.first.map(String.init) ?? ""

        switch subcmd {
        case "start":
            appState.startP2p()
        case "id":
            if let id = appState.p2pEndpointId {
                // Show as system message
                appState.activeChannelState?.appendIfNew(ChatMessage(
                    id: UUID().uuidString,
                    from: "system",
                    text: "Your iroh P2P endpoint: \(id)",
                    isAction: false,
                    timestamp: Date(),
                    replyTo: nil
                ))
            }
        case "connect":
            if parts.count > 1 {
                appState.connectP2pPeer(String(parts[1]))
            }
        case "peers":
            let peers = appState.p2pConnectedPeers
            let msg = peers.isEmpty ? "No P2P peers connected" : "P2P peers: \(peers.joined(separator: ", "))"
            appState.activeChannelState?.appendIfNew(ChatMessage(
                id: UUID().uuidString,
                from: "system",
                text: msg,
                isAction: false,
                timestamp: Date(),
                replyTo: nil
            ))
        default:
            break
        }
    }
}

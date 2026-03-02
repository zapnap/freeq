import SwiftUI

struct MessageListView: View {
    @Environment(AppState.self) private var appState

    private var messages: [ChatMessage] {
        appState.activeChannelState?.messages ?? []
    }

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(messages) { msg in
                        if !msg.isDeleted {
                            MessageRow(message: msg)
                                .id(msg.id)
                        }
                    }
                }
                .padding(.vertical, 8)
            }
            .onChange(of: messages.count) { _, _ in
                if let last = messages.last {
                    withAnimation(.easeOut(duration: 0.15)) {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
            .onAppear {
                if let last = messages.last {
                    proxy.scrollTo(last.id, anchor: .bottom)
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
    }
}

struct MessageRow: View {
    @Environment(AppState.self) private var appState
    let message: ChatMessage

    private var isSelf: Bool {
        message.from.lowercased() == appState.nick.lowercased()
    }

    private var showHeader: Bool {
        // Show header if this is the first message or sender changed
        guard let ch = appState.activeChannelState,
              let idx = ch.messages.firstIndex(where: { $0.id == message.id }),
              idx > 0 else { return true }
        let prev = ch.messages[idx - 1]
        if prev.from != message.from { return true }
        // Also show if > 5 min gap
        return message.timestamp.timeIntervalSince(prev.timestamp) > 300
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if showHeader {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(message.from)
                        .font(.system(.body, weight: .semibold))
                        .foregroundStyle(Theme.nickColor(for: message.from))

                    Text(formatTime(message.timestamp))
                        .font(.caption)
                        .foregroundStyle(.tertiary)

                    if message.isEdited {
                        Text("(edited)")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
                .padding(.top, 6)
            }

            // Message text
            if message.isAction {
                Text("• \(message.from) \(message.text)")
                    .italic()
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            } else {
                Text(parseMessageText(message.text))
                    .textSelection(.enabled)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 1)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .contextMenu {
            messageContextMenu
        }
    }

    @ViewBuilder
    private var messageContextMenu: some View {
        Button("Copy") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(message.text, forType: .string)
        }

        if isSelf, let msgId = Optional(message.id) {
            Divider()
            Button("Edit") {
                // TODO: set editing state
            }
            Button("Delete", role: .destructive) {
                appState.sendRaw("@+draft/delete=\(msgId) TAGMSG \(appState.activeChannel ?? "")")
            }
        }

        if !isSelf {
            Button("Reply") {
                // TODO: set reply state
            }
        }
    }

    /// Parse message text into AttributedString with basic formatting.
    private func parseMessageText(_ text: String) -> AttributedString {
        var result = AttributedString(text)

        // Make URLs clickable
        let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)
        if let matches = detector?.matches(in: text, range: NSRange(text.startIndex..., in: text)) {
            for match in matches.reversed() {
                guard let range = Range(match.range, in: text),
                      let attrRange = Range(range, in: result),
                      let url = match.url else { continue }
                result[attrRange].link = url
                result[attrRange].foregroundColor = .accentColor
            }
        }

        // Bold: **text**
        result = applyFormatting(result, pattern: "\\*\\*(.+?)\\*\\*", text: text) { attributed, range in
            attributed[range].inlinePresentationIntent = .stronglyEmphasized
        }

        // Italic: *text* or _text_
        result = applyFormatting(result, pattern: "(?<![*])\\*([^*]+?)\\*(?![*])", text: text) { attributed, range in
            attributed[range].inlinePresentationIntent = .emphasized
        }

        // Code: `text`
        result = applyFormatting(result, pattern: "`([^`]+?)`", text: text) { attributed, range in
            attributed[range].font = .system(.body, design: .monospaced)
            attributed[range].backgroundColor = Color(nsColor: .quaternaryLabelColor)
        }

        return result
    }

    private func applyFormatting(
        _ attributed: AttributedString,
        pattern: String,
        text: String,
        apply: (inout AttributedString, Range<AttributedString.Index>) -> Void
    ) -> AttributedString {
        var result = attributed
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return result }
        let matches = regex.matches(in: text, range: NSRange(text.startIndex..., in: text))
        for match in matches {
            guard let range = Range(match.range, in: text),
                  let attrRange = Range(range, in: result) else { continue }
            apply(&result, attrRange)
        }
        return result
    }

    private func formatTime(_ date: Date) -> String {
        let formatter = DateFormatter()
        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            formatter.dateFormat = "HH:mm"
        } else {
            formatter.dateFormat = "MMM d, HH:mm"
        }
        return formatter.string(from: date)
    }
}

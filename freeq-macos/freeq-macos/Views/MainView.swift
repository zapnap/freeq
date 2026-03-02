import SwiftUI

/// Three-column layout: Sidebar | Messages | Detail
struct MainView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        Group {
            if appState.connectionState == .disconnected && appState.brokerToken == nil {
                ConnectView()
            } else {
                NavigationSplitView {
                    SidebarView()
                        .navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 300)
                } detail: {
                    if appState.activeChannel != nil {
                        HStack(spacing: 0) {
                            ChatView()
                            if appState.showDetailPanel {
                                DetailPanel()
                                    .frame(width: 260)
                            }
                        }
                    } else {
                        VStack(spacing: 12) {
                            Image(systemName: "bubble.left.and.bubble.right")
                                .font(.system(size: 48))
                                .foregroundStyle(.tertiary)
                            Text("Select a channel to start chatting")
                                .foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
                .toolbar {
                    ToolbarItem(placement: .navigation) {
                        connectionIndicator
                    }
                }
            }
        }
        .sheet(isPresented: Binding(
            get: { appState.showJoinSheet },
            set: { appState.showJoinSheet = $0 }
        )) {
            JoinChannelSheet()
        }
    }

    @ViewBuilder
    private var connectionIndicator: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(statusColor)
                .frame(width: 8, height: 8)

            if appState.isP2pActive {
                Image(systemName: "point.3.connected.trianglepath.dotted")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .help("P2P active via iroh")
            }

            if appState.transportType == .iroh {
                Text("iroh")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .help("Connected via iroh QUIC")
            }
        }
    }

    private var statusColor: Color {
        switch appState.connectionState {
        case .registered: .green
        case .connected: .yellow
        case .connecting: .orange
        case .disconnected: .red
        }
    }
}

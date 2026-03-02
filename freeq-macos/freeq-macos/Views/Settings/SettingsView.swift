import SwiftUI

struct SettingsView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        TabView {
            GeneralSettings()
                .tabItem {
                    Label("General", systemImage: "gear")
                }

            ConnectionSettings()
                .tabItem {
                    Label("Connection", systemImage: "network")
                }

            P2pSettings()
                .tabItem {
                    Label("P2P / iroh", systemImage: "point.3.connected.trianglepath.dotted")
                }
        }
        .frame(width: 450, height: 300)
    }
}

struct GeneralSettings: View {
    var body: some View {
        Form {
            Section("Appearance") {
                Text("Follows system appearance automatically")
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

struct ConnectionSettings: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        Form {
            Section("Server") {
                LabeledContent("Address") {
                    Text(appState.serverAddress)
                }
                LabeledContent("Transport") {
                    switch appState.transportType {
                    case .iroh: Text("iroh QUIC ✓").foregroundStyle(.green)
                    case .tls: Text("TLS")
                    case .tcp: Text("TCP")
                    }
                }
                LabeledContent("Status") {
                    Text("\(String(describing: appState.connectionState))")
                }
            }
            Section("Identity") {
                LabeledContent("Nick") {
                    Text(appState.nick.isEmpty ? "—" : appState.nick)
                }
                LabeledContent("DID") {
                    if let did = appState.authenticatedDID {
                        Text(did)
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                    } else {
                        Text("Not authenticated")
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
        .formStyle(.grouped)
    }
}

struct P2pSettings: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        Form {
            Section("iroh P2P") {
                LabeledContent("Status") {
                    if appState.isP2pActive {
                        Label("Active", systemImage: "checkmark.circle.fill")
                            .foregroundStyle(.green)
                    } else {
                        Text("Inactive")
                            .foregroundStyle(.secondary)
                    }
                }

                if let id = appState.p2pEndpointId {
                    LabeledContent("Endpoint ID") {
                        Text(id)
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                    }
                }

                LabeledContent("Connected Peers") {
                    Text("\(appState.p2pConnectedPeers.count)")
                }

                if !appState.isP2pActive {
                    Button("Start P2P") {
                        appState.startP2p()
                    }
                }
            }
        }
        .formStyle(.grouped)
    }
}

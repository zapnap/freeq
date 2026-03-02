import SwiftUI

struct ConnectView: View {
    @Environment(AppState.self) private var appState
    @State private var nick: String = ""
    @State private var isLoggingIn = false

    var body: some View {
        VStack(spacing: 32) {
            Spacer()

            // Logo area
            VStack(spacing: 12) {
                Image(systemName: "bubble.left.and.bubble.right.fill")
                    .font(.system(size: 56))
                    .foregroundStyle(.tint)

                Text("freeq")
                    .font(.system(size: 36, weight: .bold, design: .rounded))

                Text("IRC with AT Protocol identity")
                    .foregroundStyle(.secondary)
            }

            // Login
            VStack(spacing: 16) {
                Button {
                    isLoggingIn = true
                    Task {
                        do {
                            let (brokerToken, session) = try await BrokerAuth.startOAuth(
                                brokerBase: appState.authBrokerBase
                            )
                            appState.brokerToken = brokerToken
                            KeychainHelper.save(key: "brokerToken", value: brokerToken)
                            appState.pendingWebToken = session.token
                            appState.authenticatedDID = session.did
                            KeychainHelper.save(key: "did", value: session.did)
                            appState.connect(nick: session.nick)
                        } catch {
                            appState.errorMessage = "Login failed: \(error.localizedDescription)"
                        }
                        isLoggingIn = false
                    }
                } label: {
                    HStack {
                        Image(systemName: "person.badge.key.fill")
                        Text("Sign in with AT Protocol")
                    }
                    .frame(maxWidth: 280)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .disabled(isLoggingIn)

                // Guest connect
                HStack(spacing: 8) {
                    TextField("Nickname", text: $nick)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 160)
                        .onSubmit { connectGuest() }

                    Button("Connect as Guest") {
                        connectGuest()
                    }
                    .disabled(nick.isEmpty)
                }
                .foregroundStyle(.secondary)
            }

            if let error = appState.errorMessage {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
            }

            if appState.connectionState == .connecting {
                ProgressView()
                    .scaleEffect(0.8)
            }

            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private func connectGuest() {
        guard !nick.isEmpty else { return }
        appState.connect(nick: nick)
    }
}

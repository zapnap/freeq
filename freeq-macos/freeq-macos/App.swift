import SwiftUI

@main
struct FreeqApp: App {
    @State private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            MainView()
                .environment(appState)
                .frame(minWidth: 800, minHeight: 500)
                .onAppear {
                    appState.reconnectIfSaved()
                }
        }
        .windowStyle(.titleBar)
        .defaultSize(width: 1100, height: 700)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New DM…") {
                    // TODO: new DM sheet
                }
                .keyboardShortcut("n")

                Button("Join Channel…") {
                    appState.showJoinSheet = true
                }
                .keyboardShortcut("j")
            }

            CommandGroup(after: .sidebar) {
                Button("Quick Switcher") {
                    appState.showQuickSwitcher = true
                }
                .keyboardShortcut("k")

                Button("Toggle Members") {
                    appState.showDetailPanel.toggle()
                }
                .keyboardShortcut("m", modifiers: [.command, .shift])

                Divider()

                ForEach(0..<9, id: \.self) { i in
                    Button("Switch to Channel \(i + 1)") {
                        appState.switchToChannelByIndex(i)
                    }
                    .keyboardShortcut(KeyEquivalent(Character("\(i + 1)")))
                }
            }
        }

        Settings {
            SettingsView()
                .environment(appState)
        }
    }
}

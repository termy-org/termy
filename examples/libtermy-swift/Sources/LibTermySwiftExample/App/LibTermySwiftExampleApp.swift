import SwiftUI

@main
struct LibTermySwiftExampleApp: App {
    @StateObject private var tabs = TerminalTabsViewModel()

    var body: some Scene {
        WindowGroup {
            TerminalTabsView(tabs: tabs)
                .frame(minWidth: 760, minHeight: 480)
        }
        .commands {
            CommandGroup(after: .newItem) {
                Button("New Tab") {
                    tabs.newTab()
                }
                .keyboardShortcut("t", modifiers: [.command])

                Button("Close Tab") {
                    tabs.closeSelectedTab()
                }
                .keyboardShortcut("w", modifiers: [.command])

                Divider()

                Button("Send Interrupt") {
                    tabs.sendControlC()
                }
                .keyboardShortcut("c", modifiers: [.control])
            }
        }
    }
}

import SwiftUI

@main
struct LibTermySwiftExampleApp: App {
    @StateObject private var terminal = TerminalViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView(terminal: terminal)
                .frame(minWidth: 760, minHeight: 480)
        }
        .commands {
            CommandGroup(after: .newItem) {
                Button("Send Interrupt") {
                    terminal.sendControlC()
                }
                .keyboardShortcut("c", modifiers: [.control])
            }
        }
    }
}

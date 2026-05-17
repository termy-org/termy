import SwiftUI

struct TerminalTabsView: View {
    @ObservedObject var tabs: TerminalTabsViewModel

    var body: some View {
        TabView(selection: $tabs.selectedTabID) {
            ForEach(tabs.tabs) { tab in
                TerminalTabView(tab: tab)
                    .tag(tab.id)
            }
        }
    }
}

private struct TerminalTabView: View {
    @ObservedObject private var terminal: TerminalViewModel

    init(tab: TerminalTab) {
        _terminal = ObservedObject(wrappedValue: tab.terminal)
    }

    var body: some View {
        ContentView(terminal: terminal)
            .tabItem {
                Text(tabTitle)
            }
    }

    private var tabTitle: String {
        let title = terminal.title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !title.isEmpty, title != "Shell" {
            return title
        }
        if let workingDirectory = terminal.workingDirectory,
           let lastPathComponent = URL(fileURLWithPath: workingDirectory).lastPathComponent
                .nilIfEmpty {
            return lastPathComponent
        }
        return "Shell"
    }
}

private extension String {
    var nilIfEmpty: String? {
        isEmpty ? nil : self
    }
}

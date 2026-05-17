import Foundation

@MainActor
final class TerminalTab: ObservableObject, Identifiable {
    let id = UUID()
    let terminal = TerminalViewModel()
}

@MainActor
final class TerminalTabsViewModel: ObservableObject {
    @Published private(set) var tabs: [TerminalTab]
    @Published var selectedTabID: UUID

    init() {
        let firstTab = TerminalTab()
        tabs = [firstTab]
        selectedTabID = firstTab.id
    }

    var selectedTab: TerminalTab? {
        tabs.first { $0.id == selectedTabID }
    }

    func newTab() {
        let tab = TerminalTab()
        tabs.append(tab)
        selectedTabID = tab.id
    }

    func closeSelectedTab() {
        guard let index = tabs.firstIndex(where: { $0.id == selectedTabID }) else {
            return
        }

        tabs[index].terminal.stop()
        tabs.remove(at: index)

        if tabs.isEmpty {
            newTab()
            return
        }

        let nextIndex = min(index, tabs.count - 1)
        selectedTabID = tabs[nextIndex].id
    }

    func sendControlC() {
        selectedTab?.terminal.sendControlC()
    }

    func searchSelected(_ query: String) -> [TerminalSearchMatch] {
        selectedTab?.terminal.search(query) ?? []
    }
}

import Foundation

enum TerminalSplitAxis: Equatable {
    case horizontal
    case vertical
}

@MainActor
final class TerminalPane: ObservableObject, Identifiable {
    let id = UUID()
    let terminal = TerminalViewModel()
}

@MainActor
final class TerminalPaneNode: ObservableObject, Identifiable {
    enum Kind {
        case leaf(TerminalPane)
        case split(axis: TerminalSplitAxis, first: TerminalPaneNode, second: TerminalPaneNode)
    }

    let id = UUID()
    @Published var kind: Kind

    init(kind: Kind) {
        self.kind = kind
    }

    static func leaf(_ pane: TerminalPane = TerminalPane()) -> TerminalPaneNode {
        TerminalPaneNode(kind: .leaf(pane))
    }
}

@MainActor
final class TerminalWorkspaceStore: ObservableObject {
    @Published private(set) var root: TerminalPaneNode
    @Published var focusedPaneID: UUID
    @Published var isSearchVisible = false

    init() {
        let firstPane = TerminalPane()
        root = TerminalPaneNode.leaf(firstPane)
        focusedPaneID = firstPane.id
    }

    var focusedTerminal: TerminalViewModel? {
        focusedPane?.terminal
    }

    var focusedPane: TerminalPane? {
        pane(with: focusedPaneID)
    }

    var paneCount: Int {
        leaves().count
    }

    func focus(_ pane: TerminalPane) {
        focusedPaneID = pane.id
    }

    func splitFocused(_ axis: TerminalSplitAxis) {
        guard let focusedPane = focusedPane else {
            return
        }

        let existingNode = TerminalPaneNode.leaf(focusedPane)
        let newPane = TerminalPane()
        let newNode = TerminalPaneNode.leaf(newPane)
        if replaceLeaf(
            paneID: focusedPane.id,
            in: root,
            with: TerminalPaneNode(kind: .split(axis: axis, first: existingNode, second: newNode))
        ) {
            focusedPaneID = newPane.id
            objectWillChange.send()
        }
    }

    func closeFocusedPane() {
        guard leaves().count > 1 else {
            focusedTerminal?.stop()
            root = TerminalPaneNode.leaf(TerminalPane())
            focusedPaneID = leaves().first?.id ?? UUID()
            return
        }

        guard let nextRoot = removingLeaf(paneID: focusedPaneID, from: root) else {
            return
        }
        root = nextRoot
        if pane(with: focusedPaneID) == nil, let first = leaves().first {
            focusedPaneID = first.id
        }
    }

    func closeFocusedPaneIfSplit() -> Bool {
        guard paneCount > 1 else {
            return false
        }
        closeFocusedPane()
        return true
    }

    func focusNextPane() {
        let panes = leaves()
        guard !panes.isEmpty else {
            return
        }
        guard let index = panes.firstIndex(where: { $0.id == focusedPaneID }) else {
            focusedPaneID = panes[0].id
            return
        }
        focusedPaneID = panes[(index + 1) % panes.count].id
    }

    func showSearch() {
        isSearchVisible = true
    }

    func hideSearch() {
        isSearchVisible = false
        focusedTerminal?.updateSearch("")
    }

    private func pane(with id: UUID) -> TerminalPane? {
        leaves().first { $0.id == id }
    }

    private func leaves() -> [TerminalPane] {
        collectLeaves(from: root)
    }

    private func collectLeaves(from node: TerminalPaneNode) -> [TerminalPane] {
        switch node.kind {
        case .leaf(let pane):
            return [pane]
        case .split(_, let first, let second):
            return collectLeaves(from: first) + collectLeaves(from: second)
        }
    }

    private func replaceLeaf(paneID: UUID, in node: TerminalPaneNode, with replacement: TerminalPaneNode) -> Bool {
        switch node.kind {
        case .leaf(let pane) where pane.id == paneID:
            node.kind = replacement.kind
            return true
        case .leaf:
            return false
        case .split(let axis, let first, let second):
            if replaceLeaf(paneID: paneID, in: first, with: replacement) {
                node.kind = .split(axis: axis, first: first, second: second)
                return true
            }
            if replaceLeaf(paneID: paneID, in: second, with: replacement) {
                node.kind = .split(axis: axis, first: first, second: second)
                return true
            }
            return false
        }
    }

    private func removingLeaf(paneID: UUID, from node: TerminalPaneNode) -> TerminalPaneNode? {
        switch node.kind {
        case .leaf(let pane):
            if pane.id == paneID {
                pane.terminal.stop()
                return nil
            }
            return node
        case .split(_, let first, let second):
            let nextFirst = removingLeaf(paneID: paneID, from: first)
            let nextSecond = removingLeaf(paneID: paneID, from: second)
            switch (nextFirst, nextSecond) {
            case let (first?, second?):
                return TerminalPaneNode(kind: nodeSplit(axisOf: node, first: first, second: second))
            case let (first?, nil):
                return first
            case let (nil, second?):
                return second
            case (nil, nil):
                return nil
            }
        }
    }

    private func nodeSplit(
        axisOf node: TerminalPaneNode,
        first: TerminalPaneNode,
        second: TerminalPaneNode
    ) -> TerminalPaneNode.Kind {
        if case .split(let axis, _, _) = node.kind {
            return .split(axis: axis, first: first, second: second)
        }
        return .split(axis: .horizontal, first: first, second: second)
    }
}

@MainActor
final class TerminalCommandRouter {
    static let shared = TerminalCommandRouter()

    weak var activeStore: TerminalWorkspaceStore?

    func closeFocusedPaneIfSplit() -> Bool {
        activeStore?.closeFocusedPaneIfSplit() ?? false
    }
}

struct TerminalCommandSet {
    var splitRight: () -> Void
    var splitDown: () -> Void
    var closePane: () -> Void
    var focusNextPane: () -> Void
    var showSearch: () -> Void
    var hideSearch: () -> Void
    var sendInterrupt: () -> Void
}

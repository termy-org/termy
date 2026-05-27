import Foundation

enum TerminalSplitAxis: Equatable {
    case horizontal
    case vertical
}

enum TerminalPaneDirection: Equatable {
    case left
    case right
    case up
    case down
}

@MainActor
final class TerminalPane: ObservableObject, Identifiable {
    let id: UUID
    let terminal: TerminalViewModel

    init(id: UUID = UUID(), workingDirectory: String? = nil, startupCommand: String? = nil) {
        self.id = id
        terminal = TerminalViewModel(workingDirectory: workingDirectory, startupCommand: startupCommand)
    }
}

@MainActor
final class TerminalPaneNode: ObservableObject, Identifiable {
    enum Kind {
        case leaf(TerminalPane)
        case split(axis: TerminalSplitAxis, first: TerminalPaneNode, second: TerminalPaneNode)
    }

    let id = UUID()
    @Published var kind: Kind
    @Published private(set) var splitRatio: Double

    init(kind: Kind, splitRatio: Double = 0.5) {
        self.kind = kind
        self.splitRatio = TerminalPaneNode.normalizedSplitRatio(splitRatio)
    }

    static func leaf(_ pane: TerminalPane = TerminalPane()) -> TerminalPaneNode {
        TerminalPaneNode(kind: .leaf(pane))
    }

    func setSplitRatio(_ ratio: Double) {
        splitRatio = TerminalPaneNode.normalizedSplitRatio(ratio)
    }

    private static func normalizedSplitRatio(_ ratio: Double) -> Double {
        min(max(ratio, 0.1), 0.9)
    }
}

@MainActor
final class TerminalWorkspaceStore: ObservableObject {
    @Published private(set) var root: TerminalPaneNode
    @Published var focusedPaneID: UUID
    @Published var isSearchVisible = false
    @Published var searchOptions = TerminalSearchOptions()
    @Published private(set) var zoomedPaneID: UUID?
    @Published var isCommandPaletteVisible = false

    init(initialTask: TermyTaskConfiguration? = nil) {
        let firstPane = TerminalPane(
            workingDirectory: initialTask?.workingDirectory,
            startupCommand: initialTask?.command
        )
        root = TerminalPaneNode.leaf(firstPane)
        focusedPaneID = firstPane.id
    }

    func snapshot() -> TerminalWorkspaceSnapshot {
        let panes = leaves()
        let paneIndices = Dictionary(uniqueKeysWithValues: panes.enumerated().map { index, pane in
            (pane.id, index)
        })
        let activePane = panes.firstIndex { $0.id == focusedPaneID } ?? 0
        let tab = TerminalWorkspaceTabSnapshot(
            panes: panes.map { pane in
                TerminalWorkspacePaneSnapshot(id: pane.id, title: pane.terminal.title)
            },
            layoutTree: layoutSnapshot(from: root, paneIndices: paneIndices),
            activePane: activePane,
            isSearchVisible: isSearchVisible
        )

        return TerminalWorkspaceSnapshot(tabs: [tab])
    }

    @discardableResult
    func restore(from snapshot: TerminalWorkspaceSnapshot) -> Bool {
        guard !snapshot.tabs.isEmpty else {
            return false
        }

        let tabIndex = max(0, min(snapshot.activeTab, snapshot.tabs.count - 1))
        let tab = snapshot.tabs[tabIndex]
        guard !tab.panes.isEmpty else {
            return false
        }

        let restoredPanes = tab.panes.map { TerminalPane(id: $0.id) }
        let restoredRoot = tab.layoutTree.flatMap { nodeSnapshot in
            node(from: nodeSnapshot, panes: restoredPanes)
        } ?? fallbackNode(for: restoredPanes)

        leaves().forEach { $0.terminal.stop() }
        root = restoredRoot

        let activePane = max(0, min(tab.activePane, restoredPanes.count - 1))
        focusedPaneID = restoredPanes[activePane].id
        isSearchVisible = tab.isSearchVisible
        zoomedPaneID = nil
        objectWillChange.send()
        return true
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

    var hasRunningTerminalProcess: Bool {
        leaves().contains { !$0.terminal.isExited }
    }

    var panesInStableOrder: [TerminalPane] {
        leaves()
    }

    var paneIDsInStableOrder: [UUID] {
        leaves().map(\.id)
    }

    var isZoomed: Bool {
        zoomedPaneID != nil
    }

    var zoomedPane: TerminalPane? {
        guard let zoomedPaneID else {
            return nil
        }
        return pane(with: zoomedPaneID)
    }

    func focus(_ pane: TerminalPane) {
        focusedPaneID = pane.id
    }

    func splitFocused(_ axis: TerminalSplitAxis) {
        guard let focusedPane = focusedPane else {
            return
        }

        let existingNode = TerminalPaneNode.leaf(focusedPane)
        let newPane = TerminalPane(workingDirectory: focusedPane.terminal.currentWorkingDirectory)
        let newNode = TerminalPaneNode.leaf(newPane)
        if replaceLeaf(
            paneID: focusedPane.id,
            in: root,
            with: TerminalPaneNode(kind: .split(axis: axis, first: existingNode, second: newNode))
        ) {
            focusedPaneID = newPane.id
            zoomedPaneID = nil
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
        if let zoomedID = zoomedPaneID, pane(with: zoomedID) == nil {
            zoomedPaneID = nil
        }
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

    func focusPreviousPane() {
        let panes = leaves()
        guard !panes.isEmpty else {
            return
        }
        guard let index = panes.firstIndex(where: { $0.id == focusedPaneID }) else {
            focusedPaneID = panes[0].id
            return
        }
        focusedPaneID = panes[(index + panes.count - 1) % panes.count].id
    }

    @discardableResult
    func focusPane(in direction: TerminalPaneDirection) -> Bool {
        let frames = leafFrames()
        guard
            let current = frames.first(where: { $0.pane.id == focusedPaneID }),
            let next = directionalCandidate(from: current, in: direction, candidates: frames)
        else {
            return false
        }
        focusedPaneID = next.pane.id
        return true
    }

    @discardableResult
    func resizeFocusedPane(in direction: TerminalPaneDirection, step: Double = 0.05) -> Bool {
        let axis: TerminalSplitAxis
        switch direction {
        case .left, .right:
            axis = .horizontal
        case .up, .down:
            axis = .vertical
        }

        guard resizeSplit(containing: focusedPaneID, in: root, axis: axis, direction: direction, step: step) else {
            return false
        }
        objectWillChange.send()
        return true
    }

    func toggleFocusedPaneZoom() {
        guard pane(with: focusedPaneID) != nil else {
            zoomedPaneID = nil
            return
        }
        zoomedPaneID = zoomedPaneID == focusedPaneID ? nil : focusedPaneID
    }

    func clearPaneZoom() {
        zoomedPaneID = nil
    }

    func isPaneZoomed(_ pane: TerminalPane) -> Bool {
        zoomedPaneID == pane.id
    }

    func showSearch() {
        isSearchVisible = true
    }

    func hideSearch() {
        isSearchVisible = false
        focusedTerminal?.updateSearch("", options: searchOptions)
    }

    func toggleSearchCaseSensitive() {
        searchOptions.caseSensitive.toggle()
    }

    func toggleSearchRegex() {
        searchOptions.usesRegex.toggle()
    }

    func showCommandPalette() {
        guard !TermyAppConfiguration.current.native.simpleMode else {
            return
        }
        isCommandPaletteVisible = true
    }

    func hideCommandPalette() {
        isCommandPaletteVisible = false
    }

    func toggleCommandPalette() {
        isCommandPaletteVisible ? hideCommandPalette() : showCommandPalette()
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

    private func containsPane(_ paneID: UUID, in node: TerminalPaneNode) -> Bool {
        switch node.kind {
        case .leaf(let pane):
            return pane.id == paneID
        case .split(_, let first, let second):
            return containsPane(paneID, in: first) || containsPane(paneID, in: second)
        }
    }

    private func replaceLeaf(paneID: UUID, in node: TerminalPaneNode, with replacement: TerminalPaneNode) -> Bool {
        switch node.kind {
        case .leaf(let pane) where pane.id == paneID:
            node.kind = replacement.kind
            node.setSplitRatio(replacement.splitRatio)
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
                return TerminalPaneNode(
                    kind: nodeSplit(axisOf: node, first: first, second: second),
                    splitRatio: node.splitRatio
                )
            case let (first?, nil):
                return first
            case let (nil, second?):
                return second
            case (nil, nil):
                return nil
            }
        }
    }

    private func resizeSplit(
        containing paneID: UUID,
        in node: TerminalPaneNode,
        axis targetAxis: TerminalSplitAxis,
        direction: TerminalPaneDirection,
        step: Double
    ) -> Bool {
        switch node.kind {
        case .leaf:
            return false
        case .split(let axis, let first, let second):
            if resizeSplit(
                containing: paneID,
                in: first,
                axis: targetAxis,
                direction: direction,
                step: step
            ) {
                return true
            }
            if resizeSplit(
                containing: paneID,
                in: second,
                axis: targetAxis,
                direction: direction,
                step: step
            ) {
                return true
            }
            guard axis == targetAxis,
                  containsPane(paneID, in: first) || containsPane(paneID, in: second)
            else {
                return false
            }

            node.setSplitRatio(node.splitRatio + splitRatioDelta(for: direction, step: step))
            return true
        }
    }

    private func splitRatioDelta(for direction: TerminalPaneDirection, step: Double) -> Double {
        switch direction {
        case .left, .up:
            return -abs(step)
        case .right, .down:
            return abs(step)
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

    private func layoutSnapshot(
        from node: TerminalPaneNode,
        paneIndices: [UUID: Int]
    ) -> TerminalWorkspaceLayoutNode? {
        switch node.kind {
        case .leaf(let pane):
            guard let paneIndex = paneIndices[pane.id] else {
                return nil
            }
            return .leaf(pane: paneIndex)
        case .split(let axis, let first, let second):
            guard
                let first = layoutSnapshot(from: first, paneIndices: paneIndices),
                let second = layoutSnapshot(from: second, paneIndices: paneIndices)
            else {
                return nil
            }
            return .split(axis: axis, ratio: node.splitRatio, first: first, second: second)
        }
    }

    private func node(
        from snapshot: TerminalWorkspaceLayoutNode,
        panes: [TerminalPane]
    ) -> TerminalPaneNode? {
        switch snapshot {
        case .leaf(let pane):
            guard panes.indices.contains(pane) else {
                return nil
            }
            return TerminalPaneNode.leaf(panes[pane])
        case .split(let axis, let ratio, let first, let second):
            guard
                let firstNode = node(from: first, panes: panes),
                let secondNode = node(from: second, panes: panes)
            else {
                return nil
            }
            return TerminalPaneNode(
                kind: .split(axis: axis, first: firstNode, second: secondNode),
                splitRatio: ratio
            )
        }
    }

    private func fallbackNode(for panes: [TerminalPane]) -> TerminalPaneNode {
        guard let firstPane = panes.first else {
            return TerminalPaneNode.leaf()
        }

        return panes.dropFirst().reduce(TerminalPaneNode.leaf(firstPane)) { partial, pane in
            TerminalPaneNode(
                kind: .split(
                    axis: .horizontal,
                    first: partial,
                    second: TerminalPaneNode.leaf(pane)
                )
            )
        }
    }

    private func leafFrames() -> [TerminalPaneFrame] {
        collectLeafFrames(
            from: root,
            frame: NormalizedPaneFrame(x: 0, y: 0, width: 1, height: 1)
        )
    }

    private func collectLeafFrames(
        from node: TerminalPaneNode,
        frame: NormalizedPaneFrame
    ) -> [TerminalPaneFrame] {
        switch node.kind {
        case .leaf(let pane):
            return [TerminalPaneFrame(pane: pane, frame: frame)]
        case .split(let axis, let first, let second):
            let ratio = node.splitRatio
            let firstFrame: NormalizedPaneFrame
            let secondFrame: NormalizedPaneFrame

            switch axis {
            case .horizontal:
                firstFrame = NormalizedPaneFrame(
                    x: frame.x,
                    y: frame.y,
                    width: frame.width * ratio,
                    height: frame.height
                )
                secondFrame = NormalizedPaneFrame(
                    x: frame.x + firstFrame.width,
                    y: frame.y,
                    width: frame.width - firstFrame.width,
                    height: frame.height
                )
            case .vertical:
                firstFrame = NormalizedPaneFrame(
                    x: frame.x,
                    y: frame.y,
                    width: frame.width,
                    height: frame.height * ratio
                )
                secondFrame = NormalizedPaneFrame(
                    x: frame.x,
                    y: frame.y + firstFrame.height,
                    width: frame.width,
                    height: frame.height - firstFrame.height
                )
            }

            return collectLeafFrames(from: first, frame: firstFrame)
                + collectLeafFrames(from: second, frame: secondFrame)
        }
    }

    private func directionalCandidate(
        from current: TerminalPaneFrame,
        in direction: TerminalPaneDirection,
        candidates: [TerminalPaneFrame]
    ) -> TerminalPaneFrame? {
        candidates
            .filter { candidate in
                candidate.pane.id != current.pane.id
                    && isCandidate(candidate.frame, direction: direction, from: current.frame)
            }
            .min { lhs, rhs in
                directionalScore(lhs.frame, direction: direction, from: current.frame)
                    < directionalScore(rhs.frame, direction: direction, from: current.frame)
            }
    }

    private func isCandidate(
        _ candidate: NormalizedPaneFrame,
        direction: TerminalPaneDirection,
        from current: NormalizedPaneFrame
    ) -> Bool {
        let epsilon = 0.0001
        switch direction {
        case .left:
            return candidate.maxX <= current.minX + epsilon
                && candidate.overlapsVertically(with: current)
        case .right:
            return candidate.minX >= current.maxX - epsilon
                && candidate.overlapsVertically(with: current)
        case .up:
            return candidate.maxY <= current.minY + epsilon
                && candidate.overlapsHorizontally(with: current)
        case .down:
            return candidate.minY >= current.maxY - epsilon
                && candidate.overlapsHorizontally(with: current)
        }
    }

    private func directionalScore(
        _ candidate: NormalizedPaneFrame,
        direction: TerminalPaneDirection,
        from current: NormalizedPaneFrame
    ) -> Double {
        let primaryDistance: Double
        let secondaryDistance: Double

        switch direction {
        case .left:
            primaryDistance = current.minX - candidate.maxX
            secondaryDistance = abs(current.midY - candidate.midY)
        case .right:
            primaryDistance = candidate.minX - current.maxX
            secondaryDistance = abs(current.midY - candidate.midY)
        case .up:
            primaryDistance = current.minY - candidate.maxY
            secondaryDistance = abs(current.midX - candidate.midX)
        case .down:
            primaryDistance = candidate.minY - current.maxY
            secondaryDistance = abs(current.midX - candidate.midX)
        }

        return primaryDistance * 1_000 + secondaryDistance
    }
}

private struct TerminalPaneFrame {
    let pane: TerminalPane
    let frame: NormalizedPaneFrame
}

private struct NormalizedPaneFrame {
    let x: Double
    let y: Double
    let width: Double
    let height: Double

    var minX: Double { x }
    var maxX: Double { x + width }
    var midX: Double { x + width / 2 }
    var minY: Double { y }
    var maxY: Double { y + height }
    var midY: Double { y + height / 2 }

    func overlapsHorizontally(with other: NormalizedPaneFrame) -> Bool {
        min(maxX, other.maxX) - max(minX, other.minX) > 0.0001
    }

    func overlapsVertically(with other: NormalizedPaneFrame) -> Bool {
        min(maxY, other.maxY) - max(minY, other.minY) > 0.0001
    }
}

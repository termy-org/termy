import XCTest
@testable import TermySwift

final class TermyNativeStressTests: XCTestCase {
    func testWorkspacePersistenceRoundTripsLargeSavedLayouts() throws {
        let temporaryDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("TermySwiftTests-\(UUID().uuidString)", isDirectory: true)
        let fileURL = temporaryDirectory.appendingPathComponent("native-workspace.json")
        let persistence = TerminalWorkspacePersistence(fileURL: fileURL)
        defer {
            try? FileManager.default.removeItem(at: temporaryDirectory)
        }

        let snapshot = makeWorkspace(tabCount: 10, panesPerTab: 8, bufferLinesPerPane: 120)
        try persistence.saveLastSession(snapshot)
        try persistence.saveAutosavedLayout(snapshot)

        XCTAssertEqual(try persistence.loadLastSession(), snapshot)
        XCTAssertEqual(try persistence.loadAutosavedLayout(), snapshot)

        try persistence.clearLastSession()
        XCTAssertThrowsError(try persistence.loadLastSession()) { error in
            XCTAssertEqual(
                String(describing: error),
                String(describing: TerminalWorkspacePersistenceError.missingLastSession)
            )
        }
        XCTAssertEqual(try persistence.loadAutosavedLayout(), snapshot)

        try persistence.saveAutosavedLayout(nil)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fileURL.path))
    }

    func testLargeSelectionRangesClampEdgesWithoutDroppingRows() {
        let selection = TerminalSelection(
            anchor: TerminalGridPosition(col: -20, row: -10),
            active: TerminalGridPosition(col: 120, row: 120_000)
        )

        let ranges = selection.rowRanges(cols: 80, rows: 100_000)

        XCTAssertEqual(ranges.count, 100_000)
        XCTAssertEqual(ranges.first, TerminalSelectionRowRange(row: 0, startCol: 0, endCol: 79))
        XCTAssertEqual(ranges.last, TerminalSelectionRowRange(row: 99_999, startCol: 0, endCol: 79))
    }

    func testRenderConfigurationClampsExtremeValues() {
        let configuration = TerminalRenderConfig(
            fontFamily: "Test",
            activeTheme: "termy",
            foreground: .termyForeground,
            background: .termyBackground,
            cursor: .termyCursor,
            fontSize: -20,
            lineHeight: 0.1,
            paddingX: -100,
            paddingY: -100,
            backgroundOpacity: 5,
            backgroundOpacityCells: true,
            cursorBlink: true,
            cursorStyle: .block,
            measuredCellWidth: -1,
            measuredCellHeight: -1,
            backgroundBlur: true,
            mouseScrollMultiplier: -3,
            scrollbarVisibility: .onScroll,
            scrollbarStyle: .theme,
            copyOnSelect: true,
            copyOnSelectToast: true,
            paneFocusEffect: .cinematic,
            paneFocusStrength: 42,
            chromeContrast: true
        )

        XCTAssertEqual(configuration.fontSize, 1)
        XCTAssertEqual(configuration.lineHeight, 0.8)
        XCTAssertEqual(configuration.paddingX, 0)
        XCTAssertEqual(configuration.paddingY, 0)
        XCTAssertEqual(configuration.backgroundOpacity, 1)
        XCTAssertEqual(configuration.measuredCellWidth, 1)
        XCTAssertEqual(configuration.measuredCellHeight, 1)
        XCTAssertEqual(configuration.mouseScrollMultiplier, 0)
        XCTAssertEqual(configuration.paneFocusStrength, 2)
    }
}

private func makeWorkspace(
    tabCount: Int,
    panesPerTab: Int,
    bufferLinesPerPane: Int
) -> TerminalWorkspaceSnapshot {
    let tabs = (0..<tabCount).map { tabIndex in
        let panes = (0..<panesPerTab).map { paneIndex in
            TerminalWorkspacePaneSnapshot(
                id: UUID(),
                title: "tab-\(tabIndex)-pane-\(paneIndex)",
                bufferText: (0..<bufferLinesPerPane)
                    .map { "tab \(tabIndex) pane \(paneIndex) scrollback line \($0)" }
                    .joined(separator: "\n")
            )
        }
        return TerminalWorkspaceTabSnapshot(
            panes: panes,
            layoutTree: makeBalancedLayout(0..<panes.count),
            activePane: tabIndex % panes.count,
            isSearchVisible: tabIndex.isMultiple(of: 2),
            pinned: tabIndex.isMultiple(of: 3),
            manualTitle: "workspace-\(tabIndex)"
        )
    }

    return TerminalWorkspaceSnapshot(activeTab: tabCount / 2, tabs: tabs)
}

private func makeBalancedLayout(_ range: Range<Int>) -> TerminalWorkspaceLayoutNode {
    if range.count == 1 {
        return .leaf(pane: range.lowerBound)
    }

    let midpoint = range.lowerBound + (range.count / 2)
    return .split(
        axis: range.count.isMultiple(of: 2) ? .horizontal : .vertical,
        ratio: 0.42,
        first: makeBalancedLayout(range.lowerBound..<midpoint),
        second: makeBalancedLayout(midpoint..<range.upperBound)
    )
}

import AppKit
import Foundation

@MainActor
final class TerminalViewModel: ObservableObject {
    @Published private(set) var frame: TerminalFrame = .empty
    @Published private(set) var errorMessage: String?
    @Published private(set) var configSummary = "config: loading"
    @Published private(set) var renderConfig = TerminalRenderConfig.default
    @Published private(set) var title = "Shell"
    @Published private(set) var progress = TerminalProgress.clear
    @Published private(set) var workingDirectory: String?
    @Published private(set) var isExited = false
    @Published private(set) var lastEvents: [TerminalRuntimeEvent] = []
    @Published private(set) var searchMatches: [TerminalSearchMatch] = []
    @Published private(set) var activeSearchMatchIndex = 0
    @Published var selection: TerminalSelection?

    private var terminal: LibTermyTerminal?
    private var timer: Timer?
    private var lastResize: TerminalResize?
    private var shouldExitAfterFirstRender: Bool {
        ProcessInfo.processInfo.environment["TERMY_SWIFT_EXAMPLE_EXIT_AFTER_RENDER"] == "1"
    }

    func start() {
        guard terminal == nil else {
            return
        }

        do {
            let terminal = try LibTermyTerminal()
            self.terminal = terminal
            configSummary = terminal.configSummary
            renderConfig = terminal.renderConfig
            isExited = false
            refresh(force: true)
            timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 30.0, repeats: true) {
                [weak self] _ in
                Task { @MainActor in
                    self?.refresh()
                }
            }
        } catch {
            errorMessage = String(describing: error)
        }
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        terminal = nil
        isExited = true
        progress = .clear
    }

    func sendControlC() {
        send(bytes: [3])
    }

    func sendKey(_ keyInput: TerminalKeyInput) {
        do {
            guard let bytes = try terminal?.encodeKey(keyInput), !bytes.isEmpty else {
                return
            }
            send(bytes: bytes)
        } catch {
            errorMessage = String(describing: error)
        }
    }

    func send(bytes: [UInt8]) {
        guard !bytes.isEmpty else {
            return
        }

        selection = nil
        if frame.displayOffset > 0 {
            scrollToBottom()
        }
        do {
            try terminal?.write(bytes)
            refresh(force: true)
        } catch {
            errorMessage = String(describing: error)
        }
    }

    func scrollDisplay(deltaLines: Int) {
        guard deltaLines != 0 else {
            return
        }

        let clampedDelta = max(Int(Int32.min), min(Int(Int32.max), deltaLines))
        do {
            if try terminal?.scrollDisplay(deltaLines: Int32(clampedDelta)) == true {
                refresh(force: true)
            }
        } catch {
            errorMessage = String(describing: error)
        }
    }

    func scrollToBottom() {
        do {
            if try terminal?.scrollToBottom() == true {
                refresh(force: true)
            }
        } catch {
            errorMessage = String(describing: error)
        }
    }

    func scrollToDisplayOffset(_ offset: Int) {
        let targetOffset = max(0, min(offset, frame.historySize))
        scrollDisplay(deltaLines: targetOffset - frame.displayOffset)
    }

    func updateSelection(_ selection: TerminalSelection?) {
        self.selection = selection
    }

    func copySelection() -> Bool {
        guard let text = frame.selectedText(for: selection), !text.isEmpty else {
            return false
        }

        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        return true
    }

    func search(_ query: String) -> [TerminalSearchMatch] {
        do {
            return try terminal?.search(query) ?? []
        } catch {
            errorMessage = String(describing: error)
            return []
        }
    }

    func updateSearch(_ query: String) {
        let trimmedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedQuery.isEmpty else {
            searchMatches = []
            activeSearchMatchIndex = 0
            return
        }

        let matches = search(trimmedQuery)
        searchMatches = matches
        activeSearchMatchIndex = matches.isEmpty ? 0 : min(activeSearchMatchIndex, matches.count - 1)
    }

    func selectNextSearchMatch() {
        guard !searchMatches.isEmpty else {
            return
        }
        activeSearchMatchIndex = (activeSearchMatchIndex + 1) % searchMatches.count
    }

    func selectPreviousSearchMatch() {
        guard !searchMatches.isEmpty else {
            return
        }
        activeSearchMatchIndex = (activeSearchMatchIndex + searchMatches.count - 1) % searchMatches.count
    }

    func resize(cols: Int, rows: Int, cellWidth: CGFloat, cellHeight: CGFloat) {
        let cols = max(2, min(cols, Int(UInt16.max)))
        let rows = max(2, min(rows, Int(UInt16.max)))
        let resize = TerminalResize(
            cols: UInt16(cols),
            rows: UInt16(rows),
            cellWidth: Float(cellWidth),
            cellHeight: Float(cellHeight)
        )
        guard resize != lastResize else {
            return
        }
        lastResize = resize

        do {
            try terminal?.resize(
                cols: resize.cols,
                rows: resize.rows,
                cellWidth: resize.cellWidth,
                cellHeight: resize.cellHeight
            )
            refresh(force: true)
        } catch {
            errorMessage = String(describing: error)
        }
    }

    private func refresh(force: Bool = false) {
        do {
            let events = try terminal?.drainEvents() ?? []
            handle(events)
            let hasEvents = !events.isEmpty
            let hasDamage = try terminal?.takeDamage() ?? false
            guard force || hasEvents || hasDamage else {
                return
            }

            if let nextFrame = try terminal?.snapshot() {
                frame = nextFrame
                errorMessage = nil
                terminateForSmokeTestIfNeeded(nextFrame)
            }
        } catch {
            errorMessage = String(describing: error)
        }
    }

    private func handle(_ events: [TerminalRuntimeEvent]) {
        guard !events.isEmpty else {
            return
        }

        lastEvents = events
        for event in events {
            switch event {
            case .title(let title):
                if !title.isEmpty {
                    self.title = title
                }
            case .resetTitle:
                title = "Shell"
            case .exit:
                isExited = true
                progress = .clear
            case .progress(let progress):
                self.progress = progress
            case .workingDirectory(let path):
                workingDirectory = path.isEmpty ? nil : path
            case .wakeup,
                 .bell,
                 .clipboardStore(_),
                 .shellPromptStart,
                 .shellCommandStart,
                 .shellCommandExecuting,
                 .shellCommandFinished(_):
                break
            }
        }
    }

    private func terminateForSmokeTestIfNeeded(_ frame: TerminalFrame) {
        guard shouldExitAfterFirstRender else {
            return
        }
        let hasText = frame.cells.contains { $0.renderText && $0.character != " " }
        guard hasText else {
            return
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) {
            NSApp.terminate(nil)
        }
    }
}

private struct TerminalResize: Equatable {
    var cols: UInt16
    var rows: UInt16
    var cellWidth: Float
    var cellHeight: Float
}

import AppKit
import Foundation

@MainActor
final class TerminalViewModel: ObservableObject {
    @Published private(set) var frame: TerminalFrame = .empty
    @Published private(set) var errorMessage: String?
    @Published private(set) var configSummary = "config: loading"
    @Published private(set) var renderConfig = TerminalRenderConfig.default
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
    }

    func sendControlC() {
        send(bytes: [3])
    }

    func send(bytes: [UInt8]) {
        guard !bytes.isEmpty else {
            return
        }

        selection = nil
        do {
            try terminal?.write(bytes)
            refresh(force: true)
        } catch {
            errorMessage = String(describing: error)
        }
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
            let hasEvents = try terminal?.drainEvents() ?? false
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

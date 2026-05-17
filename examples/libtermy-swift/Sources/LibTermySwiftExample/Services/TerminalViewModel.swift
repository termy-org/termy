import AppKit
import Foundation

@MainActor
final class TerminalViewModel: ObservableObject {
    @Published private(set) var frame: TerminalFrame = .empty
    @Published private(set) var errorMessage: String?
    @Published var commandText = ""

    private var terminal: LibTermyTerminal?
    private var timer: Timer?
    private var shouldExitAfterFirstRender: Bool {
        ProcessInfo.processInfo.environment["TERMY_SWIFT_EXAMPLE_EXIT_AFTER_RENDER"] == "1"
    }

    func start() {
        guard terminal == nil else {
            return
        }

        do {
            terminal = try LibTermyTerminal()
            refresh()
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

    func sendCommand() {
        let command = commandText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !command.isEmpty else {
            return
        }
        send(bytes: Array("\(command)\r".utf8))
        commandText = ""
    }

    func sendControlC() {
        send(bytes: [3])
    }

    private func send(bytes: [UInt8]) {
        do {
            try terminal?.write(bytes)
            refresh()
        } catch {
            errorMessage = String(describing: error)
        }
    }

    private func refresh() {
        do {
            try terminal?.drainEvents()
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

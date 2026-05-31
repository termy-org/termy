import AppKit
import Darwin
import Foundation

@MainActor
final class TerminalViewModel: ObservableObject {
    @Published private(set) var frame: TerminalFrame = .empty
    @Published private(set) var errorMessage: String?
    @Published private(set) var renderConfig = TerminalRenderConfig.default
    @Published private(set) var title = "Shell"
    @Published private(set) var progress = TerminalProgress.clear
    @Published private(set) var isExited = false
    @Published private(set) var currentWorkingDirectory: String?
    @Published private(set) var searchMatches: [TerminalSearchMatch] = []
    @Published private(set) var activeSearchMatchIndex = 0
    @Published private(set) var hoveredLink: TerminalFrameLink?
    @Published private(set) var debugMetrics = TerminalDebugMetrics.empty
    @Published var selection: TerminalSelection?

    private var terminal: LibTermyTerminal?
    private var timer: Timer?
    private var cadence: RefreshCadence = .active
    private var lastActivityAt = Date()
    private static let idleCadenceThreshold: TimeInterval = 0.4
    private static let liveSearchRefreshInterval: TimeInterval = 0.15
    private var lastResize: TerminalResize?
    private var settingsObserver: NSObjectProtocol?
    private var appearanceObserver: NSObjectProtocol?
    private var startupRefreshUntil: Date?
    private let initialWorkingDirectory: String?
    private let startupCommand: String?
    private let tmuxSessionHint: String
    private var configuration = TermyConfigurationStore.shared.configuration
    private var activeSearchQuery = ""
    private var activeSearchOptions = TerminalSearchOptions()
    private var lastSearchRefreshAt: Date?
    private var lastAutoCopiedSelectionText: String?
    private var renderedFrameCount = 0
    private var lastDebugSample = ProcessUsageSample.capture()

    init(
        workingDirectory: String? = nil,
        startupCommand: String? = nil,
        tmuxSessionHint: String = UUID().uuidString,
        restoredBufferText: String? = nil
    ) {
        initialWorkingDirectory = TerminalViewModel.normalizedWorkingDirectory(workingDirectory)
        self.startupCommand = TerminalViewModel.normalizedStartupCommand(startupCommand)
        self.tmuxSessionHint = tmuxSessionHint
        if let restoredBufferText = Self.normalizedRestoredBufferText(restoredBufferText) {
            frame = TerminalFrame.plainTextPreview(restoredBufferText)
        }
    }

    func start() {
        guard terminal == nil else {
            return
        }

        do {
            // An explicit startup command (deeplink/task) wins; otherwise launch
            // inside tmux when the integration is enabled.
            let effectiveStartupCommand = startupCommand
                ?? TmuxIntegration.startupCommand(sessionHint: tmuxSessionHint)
            let terminal = try LibTermyTerminal(
                workingDirectoryOverride: initialWorkingDirectory,
                startupCommand: effectiveStartupCommand
            )
            self.terminal = terminal
            renderConfig = terminal.renderConfig
            currentWorkingDirectory = initialWorkingDirectory
            isExited = false
            startupRefreshUntil = Date().addingTimeInterval(2)
            refresh(force: true)
            startRefreshTimer(.active)
            settingsObserver = NotificationCenter.default.addObserver(
                forName: .termySettingsChanged,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                Task { @MainActor in
                    self?.reloadConfiguration()
                }
            }
            appearanceObserver = DistributedNotificationCenter.default().addObserver(
                forName: Notification.Name("AppleInterfaceThemeChangedNotification"),
                object: nil,
                queue: .main
            ) { [weak self] _ in
                Task { @MainActor in
                    self?.reloadAppearance()
                }
            }
        } catch {
            report(error)
        }
    }

    /// Drives the polling cadence: 60 Hz while the terminal is actively producing
    /// output or receiving input (snappy), backing off to 15 Hz once idle to save
    /// CPU/battery. The expensive frame snapshot is still damage-gated in
    /// `refresh()`, so this only changes how often we poll the FFI for activity.
    private func startRefreshTimer(_ cadence: RefreshCadence) {
        self.cadence = cadence
        timer?.invalidate()
        let timer = Timer(timeInterval: cadence.interval, repeats: true) {
            [weak self] _ in
            Task { @MainActor in
                self?.refresh()
            }
        }
        timer.tolerance = cadence.interval * 0.2
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func noteActivity() {
        lastActivityAt = Date()
        if cadence != .active {
            startRefreshTimer(.active)
        }
    }

    private func adaptCadenceWhenIdle() {
        guard cadence == .active,
              Date().timeIntervalSince(lastActivityAt) > Self.idleCadenceThreshold
        else {
            return
        }
        startRefreshTimer(.idle)
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        if let settingsObserver {
            NotificationCenter.default.removeObserver(settingsObserver)
            self.settingsObserver = nil
        }
        if let appearanceObserver {
            DistributedNotificationCenter.default().removeObserver(appearanceObserver)
            self.appearanceObserver = nil
        }
        terminal = nil
        isExited = true
        progress = .clear
        startupRefreshUntil = nil
    }

    /// Re-read appearance settings from the config file and apply them to this
    /// live terminal: refreshed render config (font/metrics/padding/opacity) and
    /// reloaded theme palette so existing cells recolor.
    private func reloadAppearance() {
        do {
            renderConfig = try LibTermyTerminal.loadRenderConfig()
            try terminal?.reloadColors()
            refresh(force: true)
        } catch {
            report(error)
        }
    }

    private func reloadConfiguration() {
        configuration = TermyConfigurationStore.shared.reload()
        if !configuration.native.progressIndicatorEnabled {
            progress = .clear
        }
        reloadAppearance()
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
            report(error)
        }
    }

    func sendMouse(_ mouseInput: TerminalMouseInput) -> Bool {
        do {
            guard let bytes = try terminal?.encodeMouse(mouseInput), !bytes.isEmpty else {
                return false
            }
            send(bytes: bytes)
            return true
        } catch {
            report(error)
            return false
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
            noteActivity()
            refresh(force: true)
        } catch {
            report(error)
        }
    }

    func scrollDisplay(deltaLines: Int) {
        guard deltaLines != 0 else {
            return
        }

        let clampedDelta = max(Int(Int32.min), min(Int(Int32.max), deltaLines))
        refreshIfChanged {
            try terminal?.scrollDisplay(deltaLines: Int32(clampedDelta)) == true
        }
    }

    func scrollToBottom() {
        refreshIfChanged {
            try terminal?.scrollToBottom() == true
        }
    }

    func scrollToDisplayOffset(_ offset: Int) {
        let targetOffset = max(0, min(offset, frame.historySize))
        scrollDisplay(deltaLines: targetOffset - frame.displayOffset)
    }

    func scrollToTop() {
        scrollToDisplayOffset(frame.historySize)
    }

    func clearScrollback() {
        refreshIfChanged {
            try terminal?.clearScrollback() == true
        }
    }

    func updateSelection(_ selection: TerminalSelection?) {
        self.selection = selection
        guard renderConfig.copyOnSelect,
              let text = frame.selectedText(for: selection),
              !text.isEmpty
        else {
            lastAutoCopiedSelectionText = nil
            return
        }
        guard text != lastAutoCopiedSelectionText else {
            return
        }
        copy(text)
        lastAutoCopiedSelectionText = text
    }

    /// Double-click: select the word under the cursor.
    func selectWord(at position: TerminalGridPosition) {
        guard let selection = frame.wordSelection(at: position) else {
            updateSelection(nil)
            return
        }
        updateSelection(selection)
    }

    /// Triple-click: select the whole line under the cursor.
    func selectLine(at position: TerminalGridPosition) {
        updateSelection(frame.lineSelection(at: position))
    }

    /// Updates the link highlighted under the pointer. Returns true when a link
    /// is present so the view can show the pointing-hand cursor.
    @discardableResult
    func updateHoveredLink(at position: TerminalGridPosition?) -> Bool {
        let link = position.flatMap { frame.link(at: $0) }
        if link != hoveredLink {
            hoveredLink = link
        }
        return link != nil
    }

    /// Opens the link under `position` (⌘-click). Returns true if one was opened.
    func openLink(at position: TerminalGridPosition) -> Bool {
        guard let link = frame.link(at: position), let url = URL(string: link.target) else {
            return false
        }
        return NSWorkspace.shared.open(url)
    }

    func copySelection() -> Bool {
        guard let text = frame.selectedText(for: selection), !text.isEmpty else {
            return false
        }

        copy(text)
        return true
    }

    func search(
        _ query: String,
        options: TerminalSearchOptions = TerminalSearchOptions()
    ) -> [TerminalSearchMatch] {
        do {
            return try terminal?.search(query, options: options) ?? []
        } catch {
            report(error)
            return []
        }
    }

    func updateSearch(
        _ query: String,
        options: TerminalSearchOptions = TerminalSearchOptions()
    ) {
        let trimmedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedQuery.isEmpty else {
            activeSearchQuery = ""
            activeSearchOptions = options
            searchMatches = []
            activeSearchMatchIndex = 0
            lastSearchRefreshAt = nil
            return
        }

        let shouldResetActiveMatch = trimmedQuery != activeSearchQuery || options != activeSearchOptions
        activeSearchQuery = trimmedQuery
        activeSearchOptions = options
        refreshSearchMatches(resetActive: shouldResetActiveMatch, revealActive: true, force: true)
    }

    func selectNextSearchMatch() {
        guard !searchMatches.isEmpty else {
            return
        }
        activeSearchMatchIndex = (activeSearchMatchIndex + 1) % searchMatches.count
        revealActiveSearchMatch()
    }

    func selectPreviousSearchMatch() {
        guard !searchMatches.isEmpty else {
            return
        }
        activeSearchMatchIndex = (activeSearchMatchIndex + searchMatches.count - 1) % searchMatches.count
        revealActiveSearchMatch()
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
            report(error)
        }
    }

    private func refresh(force: Bool = false) {
        do {
            let events = try terminal?.drainEvents() ?? []
            handle(events)
            let hasEvents = !events.isEmpty
            let hasDamage = try terminal?.takeDamage() ?? false
            let isStartupRefresh = shouldForceStartupRefresh()

            if hasEvents || hasDamage {
                noteActivity()
            } else {
                adaptCadenceWhenIdle()
            }

            guard force || isStartupRefresh || hasEvents || hasDamage else {
                updateDebugMetrics(renderedFrame: false)
                return
            }

            if let nextFrame = try terminal?.snapshot() {
                frame = nextFrame
                errorMessage = nil
                updateDebugMetrics(renderedFrame: true)
                refreshSearchMatches(resetActive: false, revealActive: false, force: false)
            } else {
                updateDebugMetrics(renderedFrame: false)
            }
        } catch {
            report(error)
        }
    }

    private func shouldForceStartupRefresh() -> Bool {
        guard let startupRefreshUntil else {
            return false
        }
        if Date() < startupRefreshUntil {
            return true
        }
        self.startupRefreshUntil = nil
        return false
    }

    private func refreshIfChanged(_ operation: () throws -> Bool) {
        do {
            if try operation() {
                refresh(force: true)
            }
        } catch {
            report(error)
        }
    }

    private func report(_ error: Error) {
        errorMessage = String(describing: error)
    }

    private func copy(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    private func updateDebugMetrics(renderedFrame: Bool) {
        if renderedFrame {
            renderedFrameCount += 1
        }

        let nextSample = ProcessUsageSample.capture()
        let elapsed = nextSample.timestamp.timeIntervalSince(lastDebugSample.timestamp)
        guard elapsed >= 1 else {
            return
        }

        let fps = Double(renderedFrameCount) / elapsed
        let cpuDelta = max(0, nextSample.cpuTime - lastDebugSample.cpuTime)
        debugMetrics = TerminalDebugMetrics(
            framesPerSecond: fps,
            cpuPercent: min(999, (cpuDelta / elapsed) * 100),
            memoryMegabytes: nextSample.memoryMegabytes
        )
        renderedFrameCount = 0
        lastDebugSample = nextSample
    }

    private func refreshSearchMatches(resetActive: Bool, revealActive: Bool, force: Bool) {
        guard !activeSearchQuery.isEmpty else {
            searchMatches = []
            activeSearchMatchIndex = 0
            lastSearchRefreshAt = nil
            return
        }

        let now = Date()
        guard force || resetActive || revealActive || shouldRefreshLiveSearch(at: now) else {
            return
        }

        let matches = search(activeSearchQuery, options: activeSearchOptions)
        lastSearchRefreshAt = now
        searchMatches = matches
        if matches.isEmpty {
            activeSearchMatchIndex = 0
            return
        }

        activeSearchMatchIndex = resetActive ? 0 : min(activeSearchMatchIndex, matches.count - 1)
        if revealActive {
            revealActiveSearchMatch()
        }
    }

    private func shouldRefreshLiveSearch(at now: Date) -> Bool {
        guard let lastSearchRefreshAt else {
            return true
        }
        return now.timeIntervalSince(lastSearchRefreshAt) >= Self.liveSearchRefreshInterval
    }

    private func revealActiveSearchMatch() {
        guard searchMatches.indices.contains(activeSearchMatchIndex), frame.rows > 0 else {
            return
        }

        let match = searchMatches[activeSearchMatchIndex]
        let visibleTop = frame.historySize - frame.displayOffset
        let visibleBottom = visibleTop + frame.rows - 1
        let targetOffset: Int
        if match.row < visibleTop {
            targetOffset = frame.historySize - match.row
        } else if match.row > visibleBottom {
            targetOffset = frame.historySize - (match.row - frame.rows + 1)
        } else {
            return
        }

        let clampedOffset = max(0, min(frame.historySize, targetOffset))
        scrollDisplay(deltaLines: clampedOffset - frame.displayOffset)
    }

    private func handle(_ events: [TerminalRuntimeEvent]) {
        guard !events.isEmpty else {
            return
        }

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
                if configuration.native.progressIndicatorEnabled {
                    self.progress = progress
                }
            case .workingDirectory(let path):
                currentWorkingDirectory = TerminalViewModel.normalizedWorkingDirectory(path)
            case .clipboardStore(let text):
                // OSC 52: an app (tmux, vim, ssh) asked to set the system
                // clipboard. The Rust side already base64-decodes the payload.
                if !text.isEmpty {
                    copy(text)
                }
            case .wakeup,
                 .bell,
                 .shellPromptStart,
                 .shellCommandStart,
                 .shellCommandExecuting,
                 .shellCommandFinished(_):
                guard configuration.native.shellIntegrationEnabled else {
                    break
                }
                break
            }
        }
    }

    private static func normalizedWorkingDirectory(_ value: String?) -> String? {
        guard let value else {
            return nil
        }

        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty || trimmed.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) {
            return nil
        }

        return trimmed
    }

    private static func normalizedStartupCommand(_ value: String?) -> String? {
        guard let value else {
            return nil
        }

        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private static func normalizedRestoredBufferText(_ value: String?) -> String? {
        guard let value else {
            return nil
        }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    func visibleTextSnapshot() -> String {
        frame.visibleTextSnapshot()
    }

}

struct TerminalDebugMetrics: Equatable {
    var framesPerSecond: Double
    var cpuPercent: Double
    var memoryMegabytes: Double

    static let empty = TerminalDebugMetrics(
        framesPerSecond: 0,
        cpuPercent: 0,
        memoryMegabytes: 0
    )
}

private struct ProcessUsageSample {
    var timestamp: Date
    var cpuTime: TimeInterval
    var memoryMegabytes: Double

    static func capture() -> ProcessUsageSample {
        var usage = rusage()
        _ = getrusage(RUSAGE_SELF, &usage)
        let userCPU = TimeInterval(usage.ru_utime.tv_sec)
            + TimeInterval(usage.ru_utime.tv_usec) / 1_000_000
        let systemCPU = TimeInterval(usage.ru_stime.tv_sec)
            + TimeInterval(usage.ru_stime.tv_usec) / 1_000_000

        return ProcessUsageSample(
            timestamp: Date(),
            cpuTime: userCPU + systemCPU,
            memoryMegabytes: Double(max(0, usage.ru_maxrss)) / 1_048_576
        )
    }
}

private struct TerminalResize: Equatable {
    var cols: UInt16
    var rows: UInt16
    var cellWidth: Float
    var cellHeight: Float
}

private enum RefreshCadence {
    case active
    case idle

    var interval: TimeInterval {
        switch self {
        case .active:
            return 1.0 / 60.0
        case .idle:
            return 1.0 / 15.0
        }
    }
}

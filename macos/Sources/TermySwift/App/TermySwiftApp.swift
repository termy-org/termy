import AppKit
import SwiftUI

private enum AppMetadata {
    static let displayName = "TermyAlpha"
    static let bundleIdentifier = "com.lassevestergaard.TermyAlpha"
}

@MainActor
enum TermyNativeAppActions {
    static func openConfigFileInEditor() -> Bool {
        guard let configPath = TermyAppConfiguration.current.configPath, !configPath.isEmpty else {
            return false
        }

        let url = URL(fileURLWithPath: configPath)
        do {
            let directory = url.deletingLastPathComponent()
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            if !FileManager.default.fileExists(atPath: url.path) {
                try "# Termy config\n".write(to: url, atomically: true, encoding: .utf8)
            }
            return NSWorkspace.shared.open(url)
        } catch {
            return false
        }
    }

    static func prettifyConfig() -> Bool {
        do {
            try SettingsBridge.prettifyConfig()
            NotificationCenter.default.post(name: .termySettingsChanged, object: nil)
            return true
        } catch {
            return false
        }
    }

    static func showAppInfo() {
        NSApp.orderFrontStandardAboutPanel(nil)
    }

    static func restartApp() {
        let bundleURL = Bundle.main.bundleURL
        let configuration = NSWorkspace.OpenConfiguration()
        NSWorkspace.shared.openApplication(at: bundleURL, configuration: configuration) { _, _ in
            Task { @MainActor in
                NSApp.terminate(nil)
            }
        }
    }

    static func toggleNativeTabBarVisibility(for window: NSWindow?) -> Bool {
        guard let window = window ?? NSApp.keyWindow ?? NSApp.mainWindow else {
            return false
        }
        NSApp.sendAction(#selector(NSWindow.toggleTabBar(_:)), to: nil, from: window)
        return true
    }
}

@main
struct TermySwiftApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @FocusedValue(\.terminalCommands) private var terminalCommands

    var body: some Scene {
        WindowGroup(AppMetadata.displayName) {
            TerminalWorkspaceView()
                .frame(minWidth: 760, minHeight: 480)
                .background(WindowConfigurator())
        }
        .commands {
            CommandGroup(replacing: .appSettings) {
                OpenSettingsButton()
            }

            CommandGroup(replacing: .newItem) {
                Button("New Tab") {
                    if let terminalCommands {
                        terminalCommands.execute(.newTab)
                    } else {
                        NativeTabWindowManager.shared.openNativeTab()
                    }
                }
                .keyboardShortcut("t", modifiers: [.command])
            }

            CommandMenu("Terminal") {
                ForEach(1...9, id: \.self) { tabNumber in
                    Button("Select Tab \(tabNumber)") {
                        NativeTabWindowManager.shared.selectNativeTab(number: tabNumber)
                    }
                    .keyboardShortcut(KeyEquivalent(Character(String(tabNumber))), modifiers: [.command])
                }

                Divider()

                Button("Split Right") {
                    if !TerminalCommandRouter.shared.splitFocused(.horizontal) {
                        terminalCommands?.execute(.splitPaneVertical)
                    }
                }
                .keyboardShortcut("d", modifiers: [.command])
                .disabled(terminalCommands == nil)

                Button("Split Down") {
                    if !TerminalCommandRouter.shared.splitFocused(.vertical) {
                        terminalCommands?.execute(.splitPaneHorizontal)
                    }
                }
                .keyboardShortcut("d", modifiers: [.command, .shift])
                .disabled(terminalCommands == nil)

                Divider()

                Button("Close Pane or Tab") {
                    terminalCommands?.execute(.closePaneOrTab)
                }
                .keyboardShortcut("w", modifiers: [.command])
                .disabled(terminalCommands == nil)

                Button("Close Pane") {
                    terminalCommands?.execute(.closePane)
                }
                .keyboardShortcut("w", modifiers: [.command, .shift])
                .disabled(terminalCommands == nil)

                Divider()

                Button("Next Pane") {
                    terminalCommands?.execute(.focusPaneNext)
                }
                .keyboardShortcut("o", modifiers: [.command])
                .disabled(terminalCommands == nil)

                Button("Previous Pane") {
                    terminalCommands?.execute(.focusPanePrevious)
                }
                .keyboardShortcut("o", modifiers: [.command, .shift])
                .disabled(terminalCommands == nil)

                Button("Focus Pane Left") {
                    terminalCommands?.execute(.focusPane(.left))
                }
                .keyboardShortcut(.leftArrow, modifiers: [.command, .option])
                .disabled(terminalCommands == nil)

                Button("Focus Pane Right") {
                    terminalCommands?.execute(.focusPane(.right))
                }
                .keyboardShortcut(.rightArrow, modifiers: [.command, .option])
                .disabled(terminalCommands == nil)

                Button("Focus Pane Up") {
                    terminalCommands?.execute(.focusPane(.up))
                }
                .keyboardShortcut(.upArrow, modifiers: [.command, .option])
                .disabled(terminalCommands == nil)

                Button("Focus Pane Down") {
                    terminalCommands?.execute(.focusPane(.down))
                }
                .keyboardShortcut(.downArrow, modifiers: [.command, .option])
                .disabled(terminalCommands == nil)

                Divider()

                Button("Resize Pane Left") {
                    terminalCommands?.execute(.resizePane(.left))
                }
                .keyboardShortcut(.leftArrow, modifiers: [.command, .option, .shift])
                .disabled(terminalCommands == nil)

                Button("Resize Pane Right") {
                    terminalCommands?.execute(.resizePane(.right))
                }
                .keyboardShortcut(.rightArrow, modifiers: [.command, .option, .shift])
                .disabled(terminalCommands == nil)

                Button("Resize Pane Up") {
                    terminalCommands?.execute(.resizePane(.up))
                }
                .keyboardShortcut(.upArrow, modifiers: [.command, .option, .shift])
                .disabled(terminalCommands == nil)

                Button("Resize Pane Down") {
                    terminalCommands?.execute(.resizePane(.down))
                }
                .keyboardShortcut(.downArrow, modifiers: [.command, .option, .shift])
                .disabled(terminalCommands == nil)

                Button("Toggle Pane Zoom") {
                    terminalCommands?.execute(.togglePaneZoom)
                }
                .keyboardShortcut(.return, modifiers: [.command])
                .disabled(terminalCommands == nil)

                Divider()

                if !TermyAppConfiguration.current.tasks.isEmpty {
                    Menu("Tasks") {
                        ForEach(TermyAppConfiguration.current.tasks) { task in
                            Button(task.name) {
                                NativeTabWindowManager.shared.openNativeTab(startupTask: task)
                            }
                        }
                    }

                    Divider()
                }

                Button("Send Interrupt") {
                    terminalCommands?.execute(.sendInterrupt)
                }
                .keyboardShortcut("c", modifiers: [.control])
                .disabled(terminalCommands == nil)
            }

            CommandGroup(after: .textEditing) {
                Button("Find") {
                    terminalCommands?.execute(.openSearch)
                }
                .keyboardShortcut("f", modifiers: [.command])
                .disabled(terminalCommands == nil)

                Button("Find Next") {
                    terminalCommands?.execute(.searchNext)
                }
                .keyboardShortcut("g", modifiers: [.command])
                .disabled(terminalCommands == nil)

                Button("Find Previous") {
                    terminalCommands?.execute(.searchPrevious)
                }
                .keyboardShortcut("g", modifiers: [.command, .shift])
                .disabled(terminalCommands == nil)

                Button("Case Sensitive") {
                    terminalCommands?.execute(.toggleSearchCaseSensitive)
                }
                .keyboardShortcut("c", modifiers: [.command, .option])
                .disabled(terminalCommands == nil)

                Button("Regex") {
                    terminalCommands?.execute(.toggleSearchRegex)
                }
                .keyboardShortcut("r", modifiers: [.command, .option])
                .disabled(terminalCommands == nil)

                Button("Close Search") {
                    terminalCommands?.execute(.closeSearch)
                }
                .keyboardShortcut(.escape, modifiers: [])
                .disabled(terminalCommands == nil)
            }
        }

        Window("\(AppMetadata.displayName) Settings", id: Self.settingsWindowID) {
            SettingsRootView()
        }
        .defaultSize(width: 860, height: 600)
        .windowResizability(.contentMinSize)
    }

    static let settingsWindowID = "termy-settings"
}

/// Opens settings in a dedicated window while preserving the standard shortcut.
private struct OpenSettingsButton: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Button("Settings…") {
            if TermyAppConfiguration.current.native.simpleMode,
               TermyNativeAppActions.openConfigFileInEditor() {
                NSApp.activate(ignoringOtherApps: true)
            } else {
                openWindow(id: TermySwiftApp.settingsWindowID)
                NSApp.activate(ignoringOtherApps: true)
            }
        }
        .keyboardShortcut(",", modifiers: .command)
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var closePaneEventMonitor: LocalEventMonitor?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSWindow.allowsAutomaticWindowTabbing = true
        if let monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown, handler: { event in
            if ConfiguredKeybindRouter.shared.handle(event) {
                return nil
            }

            guard event.modifierFlags.contains(.command),
                  !event.modifierFlags.contains(.shift),
                  event.charactersIgnoringModifiers?.lowercased() == "w"
            else {
                return event
            }

            return TerminalCommandRouter.shared.closeFocusedPaneIfSplit(for: event) ? nil : event
        }) {
            closePaneEventMonitor = LocalEventMonitor(monitor)
        }
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationWillTerminate(_ notification: Notification) {
        closePaneEventMonitor?.invalidate()
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        let safety = TermySafetyConfiguration.loadCurrent()
        let hasRunningProcess = TerminalCommandRouter.shared.hasRunningTerminalProcess()
        guard safety.warnOnQuit || (safety.warnOnQuitWithRunningProcess && hasRunningProcess) else {
            return .terminateNow
        }

        let alert = NSAlert()
        alert.messageText = hasRunningProcess ? "Quit Termy with running processes?" : "Quit Termy?"
        alert.informativeText = hasRunningProcess
            ? "One or more terminal panes still have a running process."
            : "The safety setting requires confirmation before quitting."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Quit")
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn ? .terminateNow : .terminateCancel
    }

    @objc func newWindowForTab(_ sender: Any?) {
        NativeTabWindowManager.shared.openNativeTab()
    }
}

@MainActor
private final class ConfiguredKeybindRouter {
    static let shared = ConfiguredKeybindRouter()

    func handle(_ event: NSEvent) -> Bool {
        let triggers = canonicalTriggers(for: event)
        guard !triggers.isEmpty,
              let keybind = TermyAppConfiguration.current.keybinds.first(where: { triggers.contains($0.trigger) })
        else {
            return false
        }

        return execute(action: keybind.action, event: event)
    }

    private func execute(action: String, event: NSEvent) -> Bool {
        switch action {
        case "app_info":
            TermyNativeAppActions.showAppInfo()
            return true
        case "restart_app":
            TermyNativeAppActions.restartApp()
            return true
        case "open_config":
            return TermyNativeAppActions.openConfigFileInEditor()
        case "prettify_config":
            return TermyNativeAppActions.prettifyConfig()
        case "toggle_tab_bar_visibility":
            return TermyNativeAppActions.toggleNativeTabBarVisibility(for: event.window)
        case "move_tab_left":
            NativeTabWindowManager.shared.moveSelectedNativeTab(offset: -1)
            return true
        case "move_tab_right":
            NativeTabWindowManager.shared.moveSelectedNativeTab(offset: 1)
            return true
        case "switch_tab_left":
            NativeTabWindowManager.shared.selectRelativeNativeTab(offset: -1)
            return true
        case "switch_tab_right", "cycle_tabs":
            NativeTabWindowManager.shared.selectRelativeNativeTab(offset: 1)
            return true
        case "toggle_command_palette":
            guard let store = TerminalCommandRouter.shared.focusedStore(for: event),
                  !TermyAppConfiguration.current.native.simpleMode
            else {
                return false
            }
            store.toggleCommandPalette()
            return true
        case "new_tab":
            NativeTabWindowManager.shared.openNativeTab()
            return true
        case "close_tab":
            (event.window ?? NSApp.keyWindow)?.performClose(nil)
            return true
        case "close_pane_or_tab":
            if TerminalCommandRouter.shared.closeFocusedPaneIfSplit(for: event) {
                return true
            }
            (event.window ?? NSApp.keyWindow)?.performClose(nil)
            return true
        case "close_pane":
            guard let store = TerminalCommandRouter.shared.focusedStore(for: event) else {
                return false
            }
            store.closeFocusedPane()
            return true
        case "split_pane_vertical":
            return TerminalCommandRouter.shared.splitFocused(.horizontal, for: event.window)
        case "split_pane_horizontal":
            return TerminalCommandRouter.shared.splitFocused(.vertical, for: event.window)
        case "focus_pane_next":
            guard let store = TerminalCommandRouter.shared.focusedStore(for: event) else {
                return false
            }
            store.focusNextPane()
            return true
        case "focus_pane_previous":
            guard let store = TerminalCommandRouter.shared.focusedStore(for: event) else {
                return false
            }
            store.focusPreviousPane()
            return true
        case "focus_pane_left":
            return TerminalCommandRouter.shared.focusedStore(for: event)?.focusPane(in: .left) ?? false
        case "focus_pane_right":
            return TerminalCommandRouter.shared.focusedStore(for: event)?.focusPane(in: .right) ?? false
        case "focus_pane_up":
            return TerminalCommandRouter.shared.focusedStore(for: event)?.focusPane(in: .up) ?? false
        case "focus_pane_down":
            return TerminalCommandRouter.shared.focusedStore(for: event)?.focusPane(in: .down) ?? false
        case "resize_pane_left":
            return TerminalCommandRouter.shared.focusedStore(for: event)?.resizeFocusedPane(in: .left) ?? false
        case "resize_pane_right":
            return TerminalCommandRouter.shared.focusedStore(for: event)?.resizeFocusedPane(in: .right) ?? false
        case "resize_pane_up":
            return TerminalCommandRouter.shared.focusedStore(for: event)?.resizeFocusedPane(in: .up) ?? false
        case "resize_pane_down":
            return TerminalCommandRouter.shared.focusedStore(for: event)?.resizeFocusedPane(in: .down) ?? false
        case "toggle_pane_zoom":
            guard let store = TerminalCommandRouter.shared.focusedStore(for: event) else {
                return false
            }
            store.toggleFocusedPaneZoom()
            return true
        case "copy":
            return TerminalCommandRouter.shared.focusedStore(for: event)?.focusedTerminal?.copySelection() ?? false
        case "paste":
            guard let text = NSPasteboard.general.string(forType: .string) else {
                return false
            }
            TerminalCommandRouter.shared.focusedStore(for: event)?.focusedTerminal?.send(bytes: Array(text.utf8))
            return true
        case "open_search":
            guard let store = TerminalCommandRouter.shared.focusedStore(for: event) else {
                return false
            }
            store.showSearch()
            return true
        case "close_search":
            guard let store = TerminalCommandRouter.shared.focusedStore(for: event) else {
                return false
            }
            store.hideSearch()
            return true
        case "search_next":
            TerminalCommandRouter.shared.focusedStore(for: event)?.focusedTerminal?.selectNextSearchMatch()
            return true
        case "search_previous":
            TerminalCommandRouter.shared.focusedStore(for: event)?.focusedTerminal?.selectPreviousSearchMatch()
            return true
        case "toggle_search_case_sensitive":
            TerminalCommandRouter.shared.focusedStore(for: event)?.toggleSearchCaseSensitive()
            return true
        case "toggle_search_regex":
            TerminalCommandRouter.shared.focusedStore(for: event)?.toggleSearchRegex()
            return true
        case "switch_to_tab_1":
            NativeTabWindowManager.shared.selectNativeTab(number: 1)
            return true
        case "switch_to_tab_2":
            NativeTabWindowManager.shared.selectNativeTab(number: 2)
            return true
        case "switch_to_tab_3":
            NativeTabWindowManager.shared.selectNativeTab(number: 3)
            return true
        case "switch_to_tab_4":
            NativeTabWindowManager.shared.selectNativeTab(number: 4)
            return true
        case "switch_to_tab_5":
            NativeTabWindowManager.shared.selectNativeTab(number: 5)
            return true
        case "switch_to_tab_6":
            NativeTabWindowManager.shared.selectNativeTab(number: 6)
            return true
        case "switch_to_tab_7":
            NativeTabWindowManager.shared.selectNativeTab(number: 7)
            return true
        case "switch_to_tab_8":
            NativeTabWindowManager.shared.selectNativeTab(number: 8)
            return true
        case "switch_to_tab_9":
            NativeTabWindowManager.shared.selectNativeTab(number: 9)
            return true
        case "minimize_window":
            (event.window ?? NSApp.keyWindow)?.miniaturize(nil)
            return true
        case "quit":
            NSApp.terminate(nil)
            return true
        default:
            return false
        }
    }

    private func canonicalTriggers(for event: NSEvent) -> Set<String> {
        guard let key = keyName(for: event) else {
            return []
        }

        let flags = event.modifierFlags
        var baseModifiers: [String] = []
        if flags.contains(.control) {
            baseModifiers.append("ctrl")
        }
        if flags.contains(.option) {
            baseModifiers.append("alt")
        }
        if flags.contains(.shift) {
            baseModifiers.append("shift")
        }

        var triggers = Set<String>()
        if flags.contains(.command) {
            triggers.insert((baseModifiers + ["cmd", key]).joined(separator: "-"))
            triggers.insert((baseModifiers + ["secondary", key]).joined(separator: "-"))
        } else {
            triggers.insert((baseModifiers + [key]).joined(separator: "-"))
        }
        return triggers
    }

    private func keyName(for event: NSEvent) -> String? {
        switch event.keyCode {
        case 36, 76:
            return "enter"
        case 48:
            return "tab"
        case 53:
            return "escape"
        case 49:
            return "space"
        case 123:
            return "left"
        case 124:
            return "right"
        case 125:
            return "down"
        case 126:
            return "up"
        default:
            guard let characters = event.charactersIgnoringModifiers?.lowercased(),
                  let scalar = characters.unicodeScalars.first
            else {
                return nil
            }
            return String(scalar)
        }
    }
}

private final class LocalEventMonitor {
    private var invalidateHandler: (() -> Void)?

    init<Token>(_ token: Token) {
        invalidateHandler = {
            NSEvent.removeMonitor(token)
        }
    }

    func invalidate() {
        invalidateHandler?()
        invalidateHandler = nil
    }

    deinit {
        invalidate()
    }
}

struct WindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            if let window = view.window {
                NativeTabWindowManager.shared.configure(window)
            }
        }
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        DispatchQueue.main.async {
            if let window = view.window {
                NativeTabWindowManager.shared.configure(window)
            }
        }
    }
}

@MainActor
final class NativeTabWindowManager: NSObject, NSWindowDelegate {
    static let shared = NativeTabWindowManager()

    private var retainedWindows: [NSWindow] = []
    private var configuredWindowIDs = Set<ObjectIdentifier>()
    private let tabbingIdentifier = "\(AppMetadata.bundleIdentifier).native-tabs"

    func configure(_ window: NSWindow) {
        window.titlebarAppearsTransparent = true
        window.tabbingMode = .preferred
        window.tabbingIdentifier = tabbingIdentifier
        window.collectionBehavior.insert(.fullScreenPrimary)

        let identifier = ObjectIdentifier(window)
        guard !configuredWindowIDs.contains(identifier) else {
            return
        }
        configuredWindowIDs.insert(identifier)
        if window.title.isEmpty || window.title == "Window" {
            window.title = AppMetadata.displayName
        }
        window.setContentSize(TermyAppConfiguration.current.windowSize)
        window.center()
    }

    func openNativeTab(startupTask: TermyTaskConfiguration? = nil) {
        let window = makeWindow(startupTask: startupTask)
        retainedWindows.append(window)

        if let currentWindow = NSApp.keyWindow ?? NSApp.mainWindow {
            configure(currentWindow)
            currentWindow.addTabbedWindow(window, ordered: .above)
        }

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func selectNativeTab(number: Int) {
        let index = number - 1
        guard index >= 0,
              let sourceWindow = nativeTabSourceWindow()
        else {
            return
        }

        let tabbedWindows = nativeTabWindows(for: sourceWindow)
        guard tabbedWindows.indices.contains(index) else {
            return
        }

        let targetWindow = tabbedWindows[index]
        if targetWindow.isMiniaturized {
            targetWindow.deminiaturize(nil)
        }
        targetWindow.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func selectRelativeNativeTab(offset: Int) {
        guard offset != 0,
              let sourceWindow = nativeTabSourceWindow()
        else {
            return
        }

        let tabbedWindows = nativeTabWindows(for: sourceWindow)
        guard !tabbedWindows.isEmpty else {
            return
        }

        let selectedWindow = NSApp.keyWindow ?? NSApp.mainWindow
        let currentIndex = tabbedWindows.firstIndex { $0 === selectedWindow } ?? 0
        let targetIndex = (currentIndex + offset + tabbedWindows.count) % tabbedWindows.count
        let targetWindow = tabbedWindows[targetIndex]
        if targetWindow.isMiniaturized {
            targetWindow.deminiaturize(nil)
        }
        targetWindow.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func moveSelectedNativeTab(offset: Int) {
        guard offset != 0,
              let sourceWindow = nativeTabSourceWindow()
        else {
            return
        }

        let tabbedWindows = nativeTabWindows(for: sourceWindow)
        guard tabbedWindows.count > 1 else {
            return
        }

        let selectedWindow = NSApp.keyWindow ?? NSApp.mainWindow
        guard let currentIndex = tabbedWindows.firstIndex(where: { $0 === selectedWindow }) else {
            return
        }
        let targetIndex = max(0, min(tabbedWindows.count - 1, currentIndex + offset))
        guard targetIndex != currentIndex else {
            return
        }

        let movingWindow = tabbedWindows[currentIndex]
        let anchorWindow = tabbedWindows[targetIndex]
        anchorWindow.addTabbedWindow(movingWindow, ordered: offset < 0 ? .below : .above)
        movingWindow.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        guard let window = notification.object as? NSWindow else {
            return
        }
        retainedWindows.removeAll { $0 === window }
        configuredWindowIDs.remove(ObjectIdentifier(window))
    }

    private func makeWindow(startupTask: TermyTaskConfiguration? = nil) -> NSWindow {
        let windowSize = TermyAppConfiguration.current.windowSize
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: windowSize.width, height: windowSize.height),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.center()
        window.contentViewController = NSHostingController(rootView: TerminalWorkspaceView(initialTask: startupTask))
        window.isReleasedWhenClosed = false
        window.delegate = self
        configure(window)
        return window
    }

    private func nativeTabSourceWindow() -> NSWindow? {
        for window in [NSApp.keyWindow, NSApp.mainWindow].compactMap(\.self) {
            if isNativeTerminalTabWindow(window) {
                return window
            }
        }

        return NSApp.windows.first(where: isNativeTerminalTabWindow)
    }

    private func nativeTabWindows(for sourceWindow: NSWindow) -> [NSWindow] {
        let windows = sourceWindow.tabbedWindows ?? [sourceWindow]
        return windows.filter(isNativeTerminalTabWindow)
    }

    private func isNativeTerminalTabWindow(_ window: NSWindow) -> Bool {
        window.tabbingIdentifier == tabbingIdentifier
    }

}

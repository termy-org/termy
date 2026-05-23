import AppKit
import SwiftUI

private enum AppMetadata {
    static let displayName = "Termy Native Preview"
    static let bundleIdentifier = "com.lassevestergaard.TermyNativePreview"
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
                    terminalCommands?.execute(.splitPaneVertical)
                }
                .keyboardShortcut("d", modifiers: [.command])
                .disabled(terminalCommands == nil)

                Button("Split Down") {
                    terminalCommands?.execute(.splitPaneHorizontal)
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
            openWindow(id: TermySwiftApp.settingsWindowID)
            NSApp.activate(ignoringOtherApps: true)
        }
        .keyboardShortcut(",", modifiers: .command)
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var closePaneEventMonitor: LocalEventMonitor?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSWindow.allowsAutomaticWindowTabbing = true
        if let monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown, handler: { event in
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

    func openNativeTab() {
        let window = makeWindow()
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

    func windowWillClose(_ notification: Notification) {
        guard let window = notification.object as? NSWindow else {
            return
        }
        retainedWindows.removeAll { $0 === window }
        configuredWindowIDs.remove(ObjectIdentifier(window))
    }

    private func makeWindow() -> NSWindow {
        let windowSize = TermyAppConfiguration.current.windowSize
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: windowSize.width, height: windowSize.height),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.center()
        window.contentViewController = NSHostingController(rootView: TerminalWorkspaceView())
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

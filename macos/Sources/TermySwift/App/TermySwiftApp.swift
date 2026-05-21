import AppKit
import SwiftUI

@main
struct TermySwiftApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @FocusedValue(\.terminalCommands) private var terminalCommands

    var body: some Scene {
        WindowGroup("TermySwift") {
            TerminalWorkspaceView()
                .frame(minWidth: 760, minHeight: 480)
                .background(WindowConfigurator())
        }
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Tab") {
                    NativeTabWindowManager.shared.openNativeTab()
                }
                .keyboardShortcut("t", modifiers: [.command])
            }

            CommandMenu("Terminal") {
                Button("Split Right") {
                    terminalCommands?.splitRight()
                }
                .keyboardShortcut("d", modifiers: [.command])
                .disabled(terminalCommands == nil)

                Button("Split Down") {
                    terminalCommands?.splitDown()
                }
                .keyboardShortcut("d", modifiers: [.command, .shift])
                .disabled(terminalCommands == nil)

                Button("Close Pane") {
                    terminalCommands?.closePane()
                }
                .keyboardShortcut("w", modifiers: [.command, .shift])
                .disabled(terminalCommands == nil)

                Button("Next Pane") {
                    terminalCommands?.focusNextPane()
                }
                .keyboardShortcut("]", modifiers: [.command])
                .disabled(terminalCommands == nil)

                Divider()

                Button("Send Interrupt") {
                    terminalCommands?.sendInterrupt()
                }
                .keyboardShortcut("c", modifiers: [.control])
                .disabled(terminalCommands == nil)
            }

            CommandGroup(after: .textEditing) {
                Button("Find") {
                    terminalCommands?.showSearch()
                }
                .keyboardShortcut("f", modifiers: [.command])
                .disabled(terminalCommands == nil)

                Button("Close Search") {
                    terminalCommands?.hideSearch()
                }
                .keyboardShortcut(.escape, modifiers: [])
                .disabled(terminalCommands == nil)
            }
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var closePaneEventMonitor: Any?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSWindow.allowsAutomaticWindowTabbing = true
        closePaneEventMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            guard event.modifierFlags.contains(.command),
                  !event.modifierFlags.contains(.shift),
                  event.charactersIgnoringModifiers?.lowercased() == "w"
            else {
                return event
            }

            return TerminalCommandRouter.shared.closeFocusedPaneIfSplit() ? nil : event
        }
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationWillTerminate(_ notification: Notification) {
        if let closePaneEventMonitor {
            NSEvent.removeMonitor(closePaneEventMonitor)
        }
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
    private let tabbingIdentifier = "com.lassevestergaard.TermySwift.native-tabs"

    func configure(_ window: NSWindow) {
        window.title = "TermySwift"
        window.tabbingMode = .preferred
        window.tabbingIdentifier = tabbingIdentifier
        window.collectionBehavior.insert(.fullScreenPrimary)

        let identifier = ObjectIdentifier(window)
        guard !configuredWindowIDs.contains(identifier) else {
            return
        }
        configuredWindowIDs.insert(identifier)
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
}

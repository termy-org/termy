import AppKit
import SwiftUI

struct TerminalWorkspaceView: View {
    @StateObject private var store: TerminalWorkspaceStore
    @State private var appConfigurationError = TermyAppConfiguration.loadErrorMessage
    @State private var workspacePersistenceError: String?
    @State private var didRestoreWorkspace = false
    @State private var persistenceSaveTask: Task<Void, Never>?
    @State private var nativeTabWindow: NSWindow?
    @State private var nativeTabItems: [NativeTabSidebarItem] = []
    @State private var selectedNativeTabID: NativeTabSidebarItem.ID?
    private let workspacePersistence = TerminalWorkspacePersistence()
    private let shouldRestorePersistedWorkspace: Bool

    init(initialTask: TermyTaskConfiguration? = nil) {
        _store = StateObject(wrappedValue: TerminalWorkspaceStore(initialTask: initialTask))
        shouldRestorePersistedWorkspace = initialTask == nil
    }

    var body: some View {
        workspaceLayout
            .background(TerminalWorkspaceRoutingView(
                store: store,
                onWindowChanged: { window in
                    nativeTabWindow = window
                    refreshNativeTabSidebar()
                }
            ))
            .focusedValue(\.terminalCommands, commandSet)
            .onAppear {
                TerminalCommandRouter.shared.activate(store)
                restoreWorkspaceIfNeeded()
                refreshNativeTabSidebar()
            }
            .onDisappear {
                persistWorkspace()
            }
            .onReceive(store.objectWillChange) { _ in
                scheduleWorkspacePersistence()
            }
            .onReceive(NotificationCenter.default.publisher(for: .termyNativeTabsChanged)) { _ in
                refreshNativeTabSidebar()
            }
            .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
                refreshNativeTabSidebar()
            }
            .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
                persistWorkspace()
            }
    }

    @ViewBuilder
    private var workspaceLayout: some View {
        if TermyAppConfiguration.current.native.verticalTabs {
            HStack(spacing: 0) {
                TerminalNativeTabSidebar(
                    items: nativeTabItems,
                    selectedID: $selectedNativeTabID,
                    minimized: TermyAppConfiguration.current.native.verticalTabsMinimized,
                    onSelect: { id in
                        NativeTabWindowManager.shared.selectNativeTab(id: id)
                    },
                    onNewTab: {
                        NativeTabWindowManager.shared.openNativeTab()
                    },
                    onClose: { id in
                        NativeTabWindowManager.shared.closeNativeTab(id: id)
                    }
                )
                .frame(width: TermyAppConfiguration.current.native.verticalTabsMinimized
                    ? 54
                    : TermyAppConfiguration.current.native.verticalTabsWidth)

                Divider()

                workspaceContent
            }
        } else {
            workspaceContent
        }
    }

    private var workspaceContent: some View {
        ZStack(alignment: .topTrailing) {
            if let zoomedPane = store.zoomedPane {
                TerminalPaneLeafView(pane: zoomedPane, store: store)
            } else {
                TerminalPaneNodeView(node: store.root, store: store)
            }

            if let appConfigurationError {
                HStack(spacing: 8) {
                    Text(appConfigurationError)
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(.red)
                    Button {
                        self.appConfigurationError = nil
                    } label: {
                        Image(systemName: "xmark")
                    }
                    .buttonStyle(.borderless)
                }
                .padding(8)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
                .padding(10)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .zIndex(11)
            }

            if let workspacePersistenceError {
                HStack(spacing: 8) {
                    Text(workspacePersistenceError)
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(.orange)
                    Button {
                        self.workspacePersistenceError = nil
                    } label: {
                        Image(systemName: "xmark")
                    }
                    .buttonStyle(.borderless)
                }
                .padding(8)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
                .padding(10)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .zIndex(11)
            }

            if store.isSearchVisible, let terminal = store.focusedTerminal {
                TerminalSearchPanel(
                    terminal: terminal,
                    options: $store.searchOptions,
                    onClose: store.hideSearch
                )
                .padding(10)
                .zIndex(10)
            }

            if store.isCommandPaletteVisible {
                TerminalCommandPalette(
                    commandSet: commandSet,
                    onClose: store.hideCommandPalette
                )
                .padding(.top, 48)
                .zIndex(12)
            }
        }
    }

    private var commandSet: TerminalCommandSet {
        TerminalCommandSet(
            newTab: {
                NativeTabWindowManager.shared.openNativeTab()
            },
            closePaneOrTab: {
                if !store.closeFocusedPaneIfSplit() {
                    NSApp.keyWindow?.performClose(nil)
                }
                scheduleWorkspacePersistence()
            },
            splitRight: {
                store.splitFocused(.horizontal)
                scheduleWorkspacePersistence()
            },
            splitDown: {
                store.splitFocused(.vertical)
                scheduleWorkspacePersistence()
            },
            closePane: {
                store.closeFocusedPane()
                scheduleWorkspacePersistence()
            },
            focusPane: { direction in
                _ = store.focusPane(in: direction)
            },
            focusNextPane: store.focusNextPane,
            focusPreviousPane: store.focusPreviousPane,
            resizePane: { direction in
                if store.resizeFocusedPane(in: direction) {
                    scheduleWorkspacePersistence()
                }
            },
            togglePaneZoom: store.toggleFocusedPaneZoom,
            copy: {
                store.focusedTerminal?.copySelection() ?? false
            },
            paste: {
                guard let text = NSPasteboard.general.string(forType: .string) else {
                    return
                }
                store.focusedTerminal?.send(bytes: Array(text.utf8))
            },
            clearScrollback: {
                store.focusedTerminal?.clearScrollback()
            },
            showSearch: store.showSearch,
            hideSearch: store.hideSearch,
            searchNext: {
                store.focusedTerminal?.selectNextSearchMatch()
            },
            searchPrevious: {
                store.focusedTerminal?.selectPreviousSearchMatch()
            },
            toggleSearchCaseSensitive: {
                store.toggleSearchCaseSensitive()
            },
            toggleSearchRegex: {
                store.toggleSearchRegex()
            },
            sendInterrupt: { store.focusedTerminal?.sendControlC() },
            toggleCommandPalette: store.toggleCommandPalette
        )
    }

    private func restoreWorkspaceIfNeeded() {
        guard !didRestoreWorkspace else {
            return
        }
        didRestoreWorkspace = true
        guard TermyAppConfiguration.current.native.nativeTabPersistence else {
            workspacePersistenceError = nil
            return
        }
        guard shouldRestorePersistedWorkspace else {
            workspacePersistenceError = nil
            return
        }

        do {
            let snapshot = try workspacePersistence.loadLastSession()
            if store.restore(from: snapshot) {
                workspacePersistenceError = nil
            }
        } catch TerminalWorkspacePersistenceError.missingLastSession {
            workspacePersistenceError = nil
        } catch {
            workspacePersistenceError = "Could not restore workspace: \(error)"
        }
    }

    private func scheduleWorkspacePersistence() {
        guard didRestoreWorkspace,
              TermyAppConfiguration.current.native.nativeTabPersistence
        else {
            return
        }
        persistenceSaveTask?.cancel()
        persistenceSaveTask = Task {
            try? await Task.sleep(nanoseconds: 250_000_000)
            guard !Task.isCancelled else {
                return
            }
            await MainActor.run {
                persistWorkspace()
            }
        }
    }

    private func persistWorkspace() {
        guard didRestoreWorkspace,
              TermyAppConfiguration.current.native.nativeTabPersistence
        else {
            return
        }
        do {
            try workspacePersistence.saveLastSession(store.snapshot())
            workspacePersistenceError = nil
        } catch {
            workspacePersistenceError = "Could not save workspace: \(error)"
        }
    }

    private func refreshNativeTabSidebar() {
        let items = NativeTabWindowManager.shared.sidebarItems(for: nativeTabWindow)
        nativeTabItems = items
        selectedNativeTabID = items.first(where: \.isSelected)?.id ?? items.first?.id
    }
}

private struct TerminalCommandPalette: View {
    let commandSet: TerminalCommandSet
    let onClose: () -> Void

    @State private var query = ""
    @FocusState private var isSearchFocused: Bool

    private var filteredCommands: [PaletteCommand] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else {
            return paletteCommands
        }
        return paletteCommands.filter { command in
            command.title.lowercased().contains(needle)
                || command.action.lowercased().contains(needle)
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "command")
                    .foregroundStyle(.secondary)
                TextField("Command", text: $query)
                    .textFieldStyle(.plain)
                    .focused($isSearchFocused)
                    .onSubmit {
                        execute(filteredCommands.first)
                    }
                Button {
                    onClose()
                } label: {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.borderless)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)

            Divider()

            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(filteredCommands) { command in
                        Button {
                            execute(command)
                        } label: {
                            HStack(spacing: 10) {
                                Image(systemName: command.systemImage)
                                    .foregroundStyle(.secondary)
                                    .frame(width: 18)

                                Text(command.title)
                                    .lineLimit(1)

                                Spacer()

                                if TermyAppConfiguration.current.native.commandPaletteShowKeybinds,
                                   let shortcut = shortcutLabel(for: command.action) {
                                    Text(shortcut)
                                        .font(.caption.monospaced())
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .frame(maxHeight: 320)
        }
        .frame(width: 430)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .stroke(.separator.opacity(0.8), lineWidth: 1)
        }
        .shadow(radius: 18)
        .onAppear {
            isSearchFocused = true
        }
    }

    private func execute(_ command: PaletteCommand?) {
        guard let command else {
            return
        }
        onClose()
        command.execute(commandSet)
    }

    private func shortcutLabel(for action: String) -> String? {
        guard let keybind = TermyAppConfiguration.current.keybinds.first(where: { $0.action == action }) else {
            return nil
        }
        return keybind.trigger
            .replacingOccurrences(of: "secondary", with: "cmd")
            .replacingOccurrences(of: "cmd", with: "⌘")
            .replacingOccurrences(of: "ctrl", with: "⌃")
            .replacingOccurrences(of: "alt", with: "⌥")
            .replacingOccurrences(of: "shift", with: "⇧")
            .replacingOccurrences(of: "-", with: " ")
    }

    private var paletteCommands: [PaletteCommand] {
        [
            PaletteCommand(title: "New Tab", action: "new_tab", systemImage: "plus") { $0.execute(.newTab) },
            PaletteCommand(title: "Switch Tab Left", action: "switch_tab_left", systemImage: "chevron.left") { _ in
                NativeTabWindowManager.shared.selectRelativeNativeTab(offset: -1)
            },
            PaletteCommand(title: "Switch Tab Right", action: "switch_tab_right", systemImage: "chevron.right") { _ in
                NativeTabWindowManager.shared.selectRelativeNativeTab(offset: 1)
            },
            PaletteCommand(title: "Move Tab Left", action: "move_tab_left", systemImage: "arrow.left.to.line") { _ in
                NativeTabWindowManager.shared.moveSelectedNativeTab(offset: -1)
            },
            PaletteCommand(title: "Move Tab Right", action: "move_tab_right", systemImage: "arrow.right.to.line") { _ in
                NativeTabWindowManager.shared.moveSelectedNativeTab(offset: 1)
            },
            PaletteCommand(title: "Split Right", action: "split_pane_vertical", systemImage: "rectangle.split.2x1") { $0.execute(.splitPaneVertical) },
            PaletteCommand(title: "Split Down", action: "split_pane_horizontal", systemImage: "rectangle.split.1x2") { $0.execute(.splitPaneHorizontal) },
            PaletteCommand(title: "Close Pane or Tab", action: "close_pane_or_tab", systemImage: "xmark") { $0.execute(.closePaneOrTab) },
            PaletteCommand(title: "Close Pane", action: "close_pane", systemImage: "rectangle.badge.xmark") { $0.execute(.closePane) },
            PaletteCommand(title: "Next Pane", action: "focus_pane_next", systemImage: "arrow.right") { $0.execute(.focusPaneNext) },
            PaletteCommand(title: "Previous Pane", action: "focus_pane_previous", systemImage: "arrow.left") { $0.execute(.focusPanePrevious) },
            PaletteCommand(title: "Toggle Pane Zoom", action: "toggle_pane_zoom", systemImage: "arrow.up.left.and.arrow.down.right") { $0.execute(.togglePaneZoom) },
            PaletteCommand(title: "Find", action: "open_search", systemImage: "magnifyingglass") { $0.execute(.openSearch) },
            PaletteCommand(title: "Find Next", action: "search_next", systemImage: "chevron.down") { $0.execute(.searchNext) },
            PaletteCommand(title: "Find Previous", action: "search_previous", systemImage: "chevron.up") { $0.execute(.searchPrevious) },
            PaletteCommand(title: "Toggle Case Sensitive Search", action: "toggle_search_case_sensitive", systemImage: "textformat") { $0.execute(.toggleSearchCaseSensitive) },
            PaletteCommand(title: "Toggle Regex Search", action: "toggle_search_regex", systemImage: "asterisk") { $0.execute(.toggleSearchRegex) },
            PaletteCommand(title: "Copy", action: "copy", systemImage: "doc.on.doc") { $0.execute(.copy) },
            PaletteCommand(title: "Paste", action: "paste", systemImage: "doc.on.clipboard") { $0.execute(.paste) },
            PaletteCommand(title: "Clear Scrollback", action: "clear_buffer", systemImage: "trash") { $0.execute(.clearScrollback) },
            PaletteCommand(title: "Send Interrupt", action: "send_interrupt", systemImage: "exclamationmark.octagon") { $0.execute(.sendInterrupt) },
            PaletteCommand(title: "Open Config", action: "open_config", systemImage: "doc.text") { _ in
                _ = TermyNativeAppActions.openConfigFileInEditor()
            },
            PaletteCommand(title: "Prettify Config", action: "prettify_config", systemImage: "wand.and.stars") { _ in
                _ = TermyNativeAppActions.prettifyConfig()
            },
            PaletteCommand(title: "Toggle Native Tab Bar", action: "toggle_tab_bar_visibility", systemImage: "sidebar.left") { _ in
                _ = TermyNativeAppActions.toggleNativeTabBarVisibility(for: NSApp.keyWindow)
            },
            PaletteCommand(title: "App Info", action: "app_info", systemImage: "info.circle") { _ in
                TermyNativeAppActions.showAppInfo()
            },
            PaletteCommand(title: "Restart App", action: "restart_app", systemImage: "arrow.clockwise") { _ in
                TermyNativeAppActions.restartApp()
            },
        ] + TermyAppConfiguration.current.tasks.map { task in
            PaletteCommand(title: "Run \(task.name)", action: "run_task", systemImage: "play") { _ in
                NativeTabWindowManager.shared.openNativeTab(startupTask: task)
            }
        }
    }
}

private struct PaletteCommand: Identifiable {
    let id = UUID()
    let title: String
    let action: String
    let systemImage: String
    let execute: (TerminalCommandSet) -> Void
}

private struct TerminalWorkspaceRoutingView: NSViewRepresentable {
    @ObservedObject var store: TerminalWorkspaceStore
    let onWindowChanged: (NSWindow?) -> Void

    func makeNSView(context: Context) -> RoutingRegistrationView {
        RoutingRegistrationView(store: store, onWindowChanged: onWindowChanged)
    }

    func updateNSView(_ view: RoutingRegistrationView, context: Context) {
        view.store = store
        view.onWindowChanged = onWindowChanged
        view.registerCurrentWindow()
    }
}

private final class RoutingRegistrationView: NSView {
    weak var store: TerminalWorkspaceStore?
    private weak var registeredWindow: NSWindow?
    var onWindowChanged: (NSWindow?) -> Void

    init(store: TerminalWorkspaceStore, onWindowChanged: @escaping (NSWindow?) -> Void) {
        self.store = store
        self.onWindowChanged = onWindowChanged
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        onWindowChanged(window)
        registerCurrentWindow()
    }

    func registerCurrentWindow() {
        if let registeredWindow, registeredWindow !== window {
            TerminalCommandRouter.shared.unregister(window: registeredWindow)
            self.registeredWindow = nil
        }

        guard let window, let store else {
            onWindowChanged(window)
            return
        }
        registeredWindow = window
        TerminalCommandRouter.shared.register(store, for: window)
        onWindowChanged(window)
    }
}

private struct TerminalNativeTabSidebar: View {
    let items: [NativeTabSidebarItem]
    @Binding var selectedID: NativeTabSidebarItem.ID?
    let minimized: Bool
    let onSelect: (NativeTabSidebarItem.ID) -> Void
    let onNewTab: () -> Void
    let onClose: (NativeTabSidebarItem.ID) -> Void

    var body: some View {
        VStack(spacing: 0) {
            List(selection: $selectedID) {
                ForEach(items) { item in
                    NativeTabSidebarRow(item: item, minimized: minimized)
                        .tag(item.id)
                        .contextMenu {
                            Button("Close Tab") {
                                onClose(item.id)
                            }
                        }
                }
            }
            .listStyle(.sidebar)
            .onChange(of: selectedID) { _, id in
                guard let id else {
                    return
                }
                onSelect(id)
            }

            Divider()

            Button {
                onNewTab()
            } label: {
                if minimized {
                    Label("New Tab", systemImage: "plus")
                        .labelStyle(.iconOnly)
                        .frame(maxWidth: .infinity, alignment: .center)
                } else {
                    Label("New Tab", systemImage: "plus")
                        .labelStyle(.titleAndIcon)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .buttonStyle(.borderless)
            .padding(.horizontal, minimized ? 8 : 12)
            .padding(.vertical, 8)
        }
    }
}

private struct NativeTabSidebarRow: View {
    let item: NativeTabSidebarItem
    let minimized: Bool

    var body: some View {
        if minimized {
            Image(systemName: item.isSelected ? "terminal.fill" : "terminal")
                .frame(maxWidth: .infinity)
                .help(item.title)
        } else {
            HStack(spacing: 10) {
                Image(systemName: item.isSelected ? "terminal.fill" : "terminal")
                    .foregroundStyle(.secondary)
                    .frame(width: 16)

                VStack(alignment: .leading, spacing: 2) {
                    Text(item.title)
                        .lineLimit(1)

                    if let subtitle = item.subtitle {
                        Text(subtitle)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
    }
}

private struct TerminalPaneNodeView: View {
    @ObservedObject var node: TerminalPaneNode
    @ObservedObject var store: TerminalWorkspaceStore

    var body: some View {
        switch node.kind {
        case .leaf(let pane):
            TerminalPaneLeafView(pane: pane, store: store)
        case .split(let axis, let first, let second):
            StableSplitView(axis: axis, ratio: node.splitRatio) {
                TerminalPaneNodeView(node: first, store: store)
            } second: {
                TerminalPaneNodeView(node: second, store: store)
            }
        }
    }
}

private struct StableSplitView<First: View, Second: View>: NSViewControllerRepresentable {
    let axis: TerminalSplitAxis
    let ratio: Double
    let first: First
    let second: Second

    init(
        axis: TerminalSplitAxis,
        ratio: Double,
        @ViewBuilder first: () -> First,
        @ViewBuilder second: () -> Second
    ) {
        self.axis = axis
        self.ratio = ratio
        self.first = first()
        self.second = second()
    }

    func makeNSViewController(context: Context) -> StableSplitViewController<First, Second> {
        StableSplitViewController(axis: axis, ratio: ratio, first: first, second: second)
    }

    func updateNSViewController(
        _ splitViewController: StableSplitViewController<First, Second>,
        context: Context
    ) {
        splitViewController.update(axis: axis, ratio: ratio, first: first, second: second)
    }
}

private final class StableSplitViewController<First: View, Second: View>: NSSplitViewController {
    private let firstHostingController: NSHostingController<First>
    private let secondHostingController: NSHostingController<Second>
    private var didApplyInitialDividerPosition = false
    private var axis: TerminalSplitAxis
    private var ratio: Double

    init(axis: TerminalSplitAxis, ratio: Double, first: First, second: Second) {
        self.axis = axis
        self.ratio = ratio
        firstHostingController = NSHostingController(rootView: first)
        secondHostingController = NSHostingController(rootView: second)
        super.init(nibName: nil, bundle: nil)

        splitView.dividerStyle = .thin
        splitView.isVertical = axis == .horizontal

        addSplitViewItem(item(for: firstHostingController))
        addSplitViewItem(item(for: secondHostingController))
        updateMinimumThickness()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func update(axis: TerminalSplitAxis, ratio: Double, first: First, second: Second) {
        firstHostingController.rootView = first
        secondHostingController.rootView = second

        if self.axis != axis {
            self.axis = axis
            splitView.isVertical = axis == .horizontal
            didApplyInitialDividerPosition = false
            updateMinimumThickness()
        }

        if abs(self.ratio - ratio) > 0.0001 {
            self.ratio = ratio
            didApplyInitialDividerPosition = false
        }
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        applyInitialDividerPositionIfNeeded()
    }

    private func item<Content: View>(for hostingController: NSHostingController<Content>) -> NSSplitViewItem {
        let item = NSSplitViewItem(viewController: hostingController)
        item.canCollapse = false
        item.holdingPriority = .defaultLow
        return item
    }

    private func updateMinimumThickness() {
        let minimum: CGFloat = axis == .horizontal ? 220 : 120
        splitViewItems.forEach { item in
            item.minimumThickness = minimum
        }
    }

    private func applyInitialDividerPositionIfNeeded() {
        guard !didApplyInitialDividerPosition else {
            return
        }

        let length = splitView.isVertical ? splitView.bounds.width : splitView.bounds.height
        guard length > 0 else {
            return
        }

        splitView.setPosition(length * ratio, ofDividerAt: 0)
        didApplyInitialDividerPosition = true
    }
}

private struct TerminalPaneLeafView: View {
    @ObservedObject var pane: TerminalPane
    @ObservedObject var store: TerminalWorkspaceStore

    var body: some View {
        TerminalSurfaceView(
            terminal: pane.terminal,
            isFocused: store.focusedPaneID == pane.id,
            showsFocusBorder: store.paneCount > 1,
            isInputEnabled: !store.isSearchVisible,
            isSearchVisible: store.isSearchVisible,
            onFocus: {
                store.focus(pane)
            },
            onSplitRight: {
                store.focus(pane)
                store.splitFocused(.horizontal)
            },
            onSplitDown: {
                store.focus(pane)
                store.splitFocused(.vertical)
            },
            onClosePane: {
                store.focus(pane)
                store.closeFocusedPane()
            },
            onClosePaneIfSplit: {
                store.focus(pane)
                return store.closeFocusedPaneIfSplit()
            },
            onFocusNextPane: store.focusNextPane,
            onShowSearch: store.showSearch,
            onDismissSearch: store.hideSearch
        )
        .id(pane.id)
        .frame(minWidth: 240, minHeight: 120)
    }
}

private struct TerminalSearchPanel: View {
    @ObservedObject var terminal: TerminalViewModel
    @Binding var options: TerminalSearchOptions
    let onClose: () -> Void

    @State private var query = ""
    @FocusState private var isFieldFocused: Bool

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)

            TextField("Search", text: $query)
                .textFieldStyle(.plain)
                .frame(width: 220)
                .focused($isFieldFocused)
                .onSubmit {
                    terminal.selectNextSearchMatch()
                }

            Text(matchSummary)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(width: 64, alignment: .trailing)

            Button {
                terminal.selectPreviousSearchMatch()
            } label: {
                Image(systemName: "chevron.up")
            }
            .buttonStyle(.borderless)
            .disabled(terminal.searchMatches.isEmpty)

            Button {
                terminal.selectNextSearchMatch()
            } label: {
                Image(systemName: "chevron.down")
            }
            .buttonStyle(.borderless)
            .disabled(terminal.searchMatches.isEmpty)

            Button {
                options.caseSensitive.toggle()
            } label: {
                Text("Aa")
                    .font(.caption.weight(.semibold))
            }
            .buttonStyle(.borderless)
            .help("Case Sensitive")
            .foregroundStyle(options.caseSensitive ? Color.accentColor : Color.secondary)

            Button {
                options.usesRegex.toggle()
            } label: {
                Text(".*")
                    .font(.caption.monospaced().weight(.semibold))
            }
            .buttonStyle(.borderless)
            .help("Regex")
            .foregroundStyle(options.usesRegex ? Color.accentColor : Color.secondary)

            Button {
                onClose()
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.borderless)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .stroke(.separator.opacity(0.8), lineWidth: 1)
        }
        .onAppear {
            isFieldFocused = true
            terminal.updateSearch(query, options: options)
        }
        .onChange(of: query) { _, value in
            terminal.updateSearch(value, options: options)
        }
        .onChange(of: options) { _, value in
            terminal.updateSearch(query, options: value)
        }
    }

    private var matchSummary: String {
        guard !query.isEmpty else {
            return "0/0"
        }
        guard !terminal.searchMatches.isEmpty else {
            return "0/0"
        }
        return "\(terminal.activeSearchMatchIndex + 1)/\(terminal.searchMatches.count)"
    }
}

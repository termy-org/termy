import AppKit
import SwiftUI

struct TerminalWorkspaceView: View {
    @StateObject private var store = TerminalWorkspaceStore()
    @State private var appConfigurationError = TermyAppConfiguration.loadErrorMessage
    @State private var workspacePersistenceError: String?
    @State private var didRestoreWorkspace = false
    @State private var persistenceSaveTask: Task<Void, Never>?
    private let workspacePersistence = TerminalWorkspacePersistence()

    var body: some View {
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
        }
        .background(TerminalWorkspaceRoutingView(store: store))
        .focusedValue(\.terminalCommands, commandSet)
        .onAppear {
            TerminalCommandRouter.shared.activate(store)
            restoreWorkspaceIfNeeded()
        }
        .onDisappear {
            persistWorkspace()
        }
        .onReceive(store.objectWillChange) { _ in
            scheduleWorkspacePersistence()
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            persistWorkspace()
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
            sendInterrupt: { store.focusedTerminal?.sendControlC() }
        )
    }

    private func restoreWorkspaceIfNeeded() {
        guard !didRestoreWorkspace else {
            return
        }
        didRestoreWorkspace = true

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
        guard didRestoreWorkspace else {
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
        guard didRestoreWorkspace else {
            return
        }
        do {
            try workspacePersistence.saveLastSession(store.snapshot())
            workspacePersistenceError = nil
        } catch {
            workspacePersistenceError = "Could not save workspace: \(error)"
        }
    }
}

private struct TerminalWorkspaceRoutingView: NSViewRepresentable {
    @ObservedObject var store: TerminalWorkspaceStore

    func makeNSView(context: Context) -> RoutingRegistrationView {
        RoutingRegistrationView(store: store)
    }

    func updateNSView(_ view: RoutingRegistrationView, context: Context) {
        view.store = store
        view.registerCurrentWindow()
    }
}

private final class RoutingRegistrationView: NSView {
    weak var store: TerminalWorkspaceStore?
    private weak var registeredWindow: NSWindow?

    init(store: TerminalWorkspaceStore) {
        self.store = store
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        registerCurrentWindow()
    }

    func registerCurrentWindow() {
        if let registeredWindow, registeredWindow !== window {
            TerminalCommandRouter.shared.unregister(window: registeredWindow)
            self.registeredWindow = nil
        }

        guard let window, let store else {
            return
        }
        registeredWindow = window
        TerminalCommandRouter.shared.register(store, for: window)
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

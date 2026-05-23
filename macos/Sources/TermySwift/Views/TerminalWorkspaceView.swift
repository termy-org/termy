import AppKit
import SwiftUI

struct TerminalWorkspaceView: View {
    @StateObject private var store = TerminalWorkspaceStore()
    @State private var appConfigurationError = TermyAppConfiguration.loadErrorMessage

    var body: some View {
        ZStack(alignment: .topTrailing) {
            TerminalPaneNodeView(node: store.root, store: store)

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

            if store.isSearchVisible, let terminal = store.focusedTerminal {
                TerminalSearchPanel(
                    terminal: terminal,
                    onClose: store.hideSearch
                )
                .padding(10)
                .zIndex(10)
            }
        }
        .focusedValue(\.terminalCommands, commandSet)
        .onAppear {
            TerminalCommandRouter.shared.activeStore = store
        }
    }

    private var commandSet: TerminalCommandSet {
        TerminalCommandSet(
            splitRight: { store.splitFocused(.horizontal) },
            splitDown: { store.splitFocused(.vertical) },
            closePane: store.closeFocusedPane,
            focusNextPane: store.focusNextPane,
            showSearch: store.showSearch,
            hideSearch: store.hideSearch,
            sendInterrupt: { store.focusedTerminal?.sendControlC() }
        )
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
            StableSplitView(axis: axis) {
                TerminalPaneNodeView(node: first, store: store)
            } second: {
                TerminalPaneNodeView(node: second, store: store)
            }
        }
    }
}

private struct StableSplitView<First: View, Second: View>: NSViewControllerRepresentable {
    let axis: TerminalSplitAxis
    let first: First
    let second: Second

    init(
        axis: TerminalSplitAxis,
        @ViewBuilder first: () -> First,
        @ViewBuilder second: () -> Second
    ) {
        self.axis = axis
        self.first = first()
        self.second = second()
    }

    func makeNSViewController(context: Context) -> StableSplitViewController<First, Second> {
        StableSplitViewController(axis: axis, first: first, second: second)
    }

    func updateNSViewController(
        _ splitViewController: StableSplitViewController<First, Second>,
        context: Context
    ) {
        splitViewController.update(axis: axis, first: first, second: second)
    }
}

private final class StableSplitViewController<First: View, Second: View>: NSSplitViewController {
    private let firstHostingController: NSHostingController<First>
    private let secondHostingController: NSHostingController<Second>
    private var didApplyInitialDividerPosition = false
    private var axis: TerminalSplitAxis

    init(axis: TerminalSplitAxis, first: First, second: Second) {
        self.axis = axis
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

    func update(axis: TerminalSplitAxis, first: First, second: Second) {
        firstHostingController.rootView = first
        secondHostingController.rootView = second

        if self.axis != axis {
            self.axis = axis
            splitView.isVertical = axis == .horizontal
            didApplyInitialDividerPosition = false
            updateMinimumThickness()
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

        splitView.setPosition(length / 2, ofDividerAt: 0)
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
            onShowSearch: store.showSearch
        )
        .frame(minWidth: 240, minHeight: 120)
    }
}

private struct TerminalSearchPanel: View {
    @ObservedObject var terminal: TerminalViewModel
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
            terminal.updateSearch(query)
        }
        .onChange(of: query) { _, value in
            terminal.updateSearch(value)
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

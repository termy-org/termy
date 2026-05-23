import SwiftUI

struct TerminalSurfaceView: View {
    @ObservedObject var terminal: TerminalViewModel
    @State private var isScrollBarVisible = false
    @State private var scrollBarHideTask: Task<Void, Never>?
    @State private var lastSize: CGSize = .zero

    let isFocused: Bool
    let showsFocusBorder: Bool
    let isInputEnabled: Bool
    let onFocus: () -> Void
    let onSplitRight: () -> Void
    let onSplitDown: () -> Void
    let onClosePane: () -> Void
    let onClosePaneIfSplit: () -> Bool
    let onFocusNextPane: () -> Void
    let onShowSearch: () -> Void

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .topLeading) {
                TerminalGridView(
                    frame: terminal.frame,
                    selection: terminal.selection,
                    renderConfig: terminal.renderConfig,
                    searchMatches: terminal.searchMatches,
                    activeSearchMatch: terminal.searchMatches[safe: terminal.activeSearchMatchIndex]
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

                TerminalKeyboardInputView(
                    cols: terminal.frame.cols,
                    rows: terminal.frame.rows,
                    renderConfig: terminal.renderConfig,
                    isFocused: isFocused,
                    isInputEnabled: isInputEnabled,
                    onFocus: onFocus,
                    onBytes: { bytes in
                        terminal.send(bytes: bytes)
                    },
                    onKeyInput: { keyInput in
                        terminal.sendKey(keyInput)
                    },
                    onScrollLines: { lines in
                        revealScrollBar()
                        terminal.scrollDisplay(deltaLines: lines)
                    },
                    onScrollToTop: {
                        revealScrollBar()
                        terminal.scrollToTop()
                    },
                    onScrollToBottom: {
                        revealScrollBar()
                        terminal.scrollToBottom()
                    },
                    onClearBuffer: {
                        terminal.clearScrollback()
                    },
                    onSplitRight: onSplitRight,
                    onSplitDown: onSplitDown,
                    onClosePane: onClosePane,
                    onClosePaneIfSplit: onClosePaneIfSplit,
                    onFocusNextPane: onFocusNextPane,
                    onShowSearch: onShowSearch,
                    onSelectionChanged: { selection in
                        terminal.updateSelection(selection)
                    },
                    onCopy: {
                        terminal.copySelection()
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)

                TerminalTopLoader(progress: terminal.progress)
                    .frame(maxWidth: .infinity, alignment: .topLeading)

                if let errorMessage = terminal.errorMessage {
                    Text(errorMessage)
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(.red)
                        .padding(8)
                        .background(.regularMaterial)
                        .padding(8)
                }
            }
            .overlay {
                if isFocused && showsFocusBorder {
                    Rectangle()
                        .stroke(Color.accentColor.opacity(0.6), lineWidth: 1)
                        .allowsHitTesting(false)
                }
            }
            .overlay(alignment: .trailing) {
                if isScrollBarVisible, terminal.frame.historySize > 0 {
                    TerminalScrollBar(
                        frame: terminal.frame,
                        onInteraction: revealScrollBar,
                        onScrollToOffset: { offset in
                            revealScrollBar()
                            terminal.scrollToDisplayOffset(offset)
                        }
                    )
                    .frame(width: 14)
                    .padding(.trailing, 3)
                    .transition(.opacity)
                }
            }
            .background(
                terminal.renderConfig.background.swiftUIColor
                    .opacity(terminal.renderConfig.backgroundOpacity)
            )
            .onTapGesture {
                if isInputEnabled {
                    onFocus()
                }
            }
            .onAppear {
                terminal.start()
                lastSize = proxy.size
                resizeTerminal(to: proxy.size)
            }
            .onChange(of: proxy.size) { _, size in
                lastSize = size
                resizeTerminal(to: size)
            }
            .onChange(of: terminal.renderConfig) { _, _ in
                // Font/padding changes alter cell metrics, so recompute the grid
                // for the current pixel size when settings live-apply.
                resizeTerminal(to: lastSize)
            }
            .onChange(of: terminal.frame.displayOffset) { oldValue, newValue in
                if oldValue != newValue {
                    revealScrollBar()
                }
            }
        }
    }

    private func resizeTerminal(to size: CGSize) {
        terminal.resize(
            cols: Int((size.width - (terminal.renderConfig.paddingX * 2))
                / terminal.renderConfig.cellWidth),
            rows: Int((size.height - (terminal.renderConfig.paddingY * 2))
                / terminal.renderConfig.cellHeight),
            cellWidth: terminal.renderConfig.cellWidth,
            cellHeight: terminal.renderConfig.cellHeight
        )
    }

    private func revealScrollBar() {
        guard terminal.frame.historySize > 0 else {
            return
        }

        isScrollBarVisible = true
        scrollBarHideTask?.cancel()
        scrollBarHideTask = Task {
            try? await Task.sleep(nanoseconds: 1_200_000_000)
            guard !Task.isCancelled else {
                return
            }
            await MainActor.run {
                isScrollBarVisible = false
            }
        }
    }
}

private struct TerminalScrollBar: View {
    let frame: TerminalFrame
    let onInteraction: () -> Void
    let onScrollToOffset: (Int) -> Void

    var body: some View {
        GeometryReader { proxy in
            if metrics(for: proxy.size).isVisible {
                ZStack(alignment: .top) {
                    Capsule()
                        .fill(Color.primary.opacity(0.10))
                        .frame(width: 7)

                    Capsule()
                        .fill(Color.primary.opacity(frame.displayOffset == 0 ? 0.34 : 0.58))
                        .frame(width: 7, height: metrics(for: proxy.size).thumbHeight)
                        .offset(y: metrics(for: proxy.size).thumbY)
                        .gesture(
                            DragGesture(minimumDistance: 0)
                                .onChanged { value in
                                    onInteraction()
                                    let metrics = metrics(for: proxy.size)
                                    guard metrics.travel > 0 else {
                                        return
                                    }
                                    let clampedY = max(0, min(value.location.y, metrics.travel))
                                    let progressFromTop = clampedY / metrics.travel
                                    let target = Int(round(CGFloat(frame.historySize) * (1 - progressFromTop)))
                                    onScrollToOffset(target)
                                }
                        )
                }
                .padding(.vertical, framePaddingY)
                .contentShape(Rectangle())
            }
        }
        .allowsHitTesting(frame.historySize > 0)
    }

    private var framePaddingY: CGFloat {
        8
    }

    private func metrics(for size: CGSize) -> ScrollBarMetrics {
        guard frame.historySize > 0, frame.rows > 0, size.height > framePaddingY * 2 else {
            return ScrollBarMetrics(isVisible: false, thumbHeight: 0, thumbY: 0, travel: 0)
        }

        let trackHeight = size.height - (framePaddingY * 2)
        let totalRows = max(1, frame.historySize + frame.rows)
        let visibleFraction = CGFloat(frame.rows) / CGFloat(totalRows)
        let thumbHeight = max(28, min(trackHeight, trackHeight * visibleFraction))
        let travel = max(0, trackHeight - thumbHeight)
        let maxOffset = max(1, frame.historySize)
        let normalizedOffset = CGFloat(max(0, min(frame.displayOffset, frame.historySize)))
            / CGFloat(maxOffset)
        let thumbY = travel * (1 - normalizedOffset)

        return ScrollBarMetrics(
            isVisible: true,
            thumbHeight: thumbHeight,
            thumbY: thumbY,
            travel: travel
        )
    }
}

private struct ScrollBarMetrics {
    var isVisible: Bool
    var thumbHeight: CGFloat
    var thumbY: CGFloat
    var travel: CGFloat
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        guard index >= 0, index < count else {
            return nil
        }
        return self[index]
    }
}

import AppKit
import SwiftUI

struct TerminalSurfaceView: View {
    @ObservedObject var terminal: TerminalViewModel
    @State private var isScrollBarVisible = false
    @State private var scrollBarHideTask: Task<Void, Never>?
    @State private var lastSize: CGSize = .zero

    let isFocused: Bool
    let showsFocusBorder: Bool
    let isInputEnabled: Bool
    let isSearchVisible: Bool
    let onFocus: () -> Void
    let onSplitRight: () -> Void
    let onSplitDown: () -> Void
    let onClosePane: () -> Void
    let onClosePaneIfSplit: () -> Bool
    let onFocusNextPane: () -> Void
    let onShowSearch: () -> Void
    let onDismissSearch: () -> Void

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .topLeading) {
                TerminalGridView(
                    frame: terminal.frame,
                    selection: terminal.selection,
                    renderConfig: terminal.renderConfig,
                    searchMatches: terminal.searchMatches,
                    activeSearchMatch: terminal.searchMatches[safe: terminal.activeSearchMatchIndex],
                    isFocused: isFocused
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

                TerminalKeyboardInputView(
                    cols: terminal.frame.cols,
                    rows: terminal.frame.rows,
                    renderConfig: terminal.renderConfig,
                    isFocused: isFocused,
                    isInputEnabled: isInputEnabled,
                    canCopy: hasCopyableSelection,
                    onFocus: onFocus,
                    onBytes: { bytes in
                        terminal.send(bytes: bytes)
                    },
                    onKeyInput: { keyInput in
                        terminal.sendKey(keyInput)
                    },
                    onMouseInput: { mouseInput in
                        terminal.sendMouse(mouseInput)
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

                if terminal.isExited && !hasVisibleTerminalContent {
                    Text("Process exited")
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .padding(8)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
            }
            .overlay {
                focusEffectOverlay
            }
            .overlay(alignment: .trailing) {
                if shouldShowScrollBar {
                    TerminalScrollBar(
                        frame: terminal.frame,
                        renderConfig: terminal.renderConfig,
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
            .background {
                if terminal.renderConfig.backgroundBlur,
                   terminal.renderConfig.backgroundOpacity < 1.0 {
                    Rectangle()
                        .fill(.ultraThinMaterial)
                }
            }
            .background(TerminalWindowChromeSyncView(
                title: terminal.title,
                background: terminal.renderConfig.background,
                chromeContrast: terminal.renderConfig.chromeContrast,
                isFocused: isFocused
            ))
            .onTapGesture {
                onFocus()
                if isSearchVisible {
                    onDismissSearch()
                }
            }
            .onAppear {
                onFocus()
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
        guard terminal.frame.historySize > 0,
              terminal.renderConfig.scrollbarVisibility != .off
        else {
            return
        }

        isScrollBarVisible = true
        guard terminal.renderConfig.scrollbarVisibility == .onScroll else {
            return
        }
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

    private var shouldShowScrollBar: Bool {
        guard terminal.frame.historySize > 0 else {
            return false
        }
        switch terminal.renderConfig.scrollbarVisibility {
        case .off:
            return false
        case .always:
            return true
        case .onScroll:
            return isScrollBarVisible
        }
    }

    private var hasCopyableSelection: Bool {
        guard let text = terminal.frame.selectedText(for: terminal.selection) else {
            return false
        }
        return !text.isEmpty
    }

    private var hasVisibleTerminalContent: Bool {
        terminal.frame.cells.contains { cell in
            cell.renderText && cell.character != " "
        }
    }

    private var focusStrength: Double {
        Double(terminal.renderConfig.paneFocusStrength)
    }

    @ViewBuilder
    private var focusEffectOverlay: some View {
        if showsFocusBorder {
            switch terminal.renderConfig.paneFocusEffect {
            case .off:
                EmptyView()
            case .minimal:
                if isFocused {
                    Rectangle()
                        .stroke(Color.accentColor.opacity(0.35 + (0.15 * focusStrength)), lineWidth: 1)
                        .allowsHitTesting(false)
                }
            case .softSpotlight:
                if isFocused {
                    Rectangle()
                        .stroke(Color.accentColor.opacity(0.45 + (0.12 * focusStrength)), lineWidth: 1)
                        .allowsHitTesting(false)
                } else {
                    Rectangle()
                        .fill(Color.black.opacity(0.08 * focusStrength))
                        .allowsHitTesting(false)
                }
            case .cinematic:
                if isFocused {
                    Rectangle()
                        .stroke(Color.accentColor.opacity(0.55 + (0.12 * focusStrength)), lineWidth: 1.25)
                        .shadow(color: Color.accentColor.opacity(0.16 * focusStrength), radius: 5)
                        .allowsHitTesting(false)
                } else {
                    Rectangle()
                        .fill(Color.black.opacity(0.14 * focusStrength))
                        .allowsHitTesting(false)
                }
            }
        }
    }
}

private struct TerminalWindowChromeSyncView: NSViewRepresentable {
    let title: String
    let background: TerminalRGBA
    let chromeContrast: Bool
    let isFocused: Bool

    func makeNSView(context: Context) -> TerminalWindowChromeSyncNSView {
        let view = TerminalWindowChromeSyncNSView(frame: .zero)
        view.onWindowAttached = { attachedView in
            syncChrome(from: attachedView)
        }
        syncChrome(from: view)
        return view
    }

    func updateNSView(_ view: TerminalWindowChromeSyncNSView, context: Context) {
        view.onWindowAttached = { attachedView in
            syncChrome(from: attachedView)
        }
        syncChrome(from: view)
    }

    private func syncChrome(from view: NSView) {
        guard isFocused else {
            return
        }
        let nextTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
        let resolvedTitle = nextTitle.isEmpty ? "Shell" : nextTitle
        let resolvedBackground = background.nsTitlebarColor(chromeContrast: chromeContrast)
        let resolvedAppearance = background.prefersDarkTitlebarAppearance
            ? NSAppearance(named: .darkAqua)
            : NSAppearance(named: .aqua)
        DispatchQueue.main.async {
            guard let window = view.window else {
                return
            }
            if window.title != resolvedTitle {
                window.title = resolvedTitle
            }
            window.titlebarAppearsTransparent = true
            window.backgroundColor = resolvedBackground
            window.appearance = resolvedAppearance
        }
    }
}

private final class TerminalWindowChromeSyncNSView: NSView {
    var onWindowAttached: ((NSView) -> Void)?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        onWindowAttached?(self)
    }
}

private extension TerminalRGBA {
    func nsTitlebarColor(chromeContrast: Bool) -> NSColor {
        let contrastMultiplier = chromeContrast ? 0.78 : 1.0
        return NSColor(
            srgbRed: red * contrastMultiplier,
            green: green * contrastMultiplier,
            blue: blue * contrastMultiplier,
            alpha: 1.0
        )
    }

    var prefersDarkTitlebarAppearance: Bool {
        let linearRed = linearizedSRGB(red)
        let linearGreen = linearizedSRGB(green)
        let linearBlue = linearizedSRGB(blue)
        let luminance = (0.2126 * linearRed) + (0.7152 * linearGreen) + (0.0722 * linearBlue)
        return luminance < 0.5
    }

    private func linearizedSRGB(_ component: Double) -> Double {
        if component <= 0.04045 {
            return component / 12.92
        }
        return pow((component + 0.055) / 1.055, 2.4)
    }
}

private struct TerminalScrollBar: View {
    let frame: TerminalFrame
    let renderConfig: TerminalRenderConfig
    let onInteraction: () -> Void
    let onScrollToOffset: (Int) -> Void

    var body: some View {
        GeometryReader { proxy in
            if metrics(for: proxy.size).isVisible {
                ZStack(alignment: .top) {
                    Capsule()
                        .fill(trackColor)
                        .frame(width: 7)

                    Capsule()
                        .fill(thumbColor)
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

    private var trackColor: Color {
        switch renderConfig.scrollbarStyle {
        case .neutral:
            return Color.primary.opacity(0.10)
        case .mutedTheme:
            return renderConfig.foreground.swiftUIColor.opacity(0.08)
        case .theme:
            return renderConfig.cursor.swiftUIColor.opacity(0.16)
        }
    }

    private var thumbColor: Color {
        let opacity = frame.displayOffset == 0 ? 0.34 : 0.58
        switch renderConfig.scrollbarStyle {
        case .neutral:
            return Color.primary.opacity(opacity)
        case .mutedTheme:
            return renderConfig.foreground.swiftUIColor.opacity(opacity * 0.75)
        case .theme:
            return renderConfig.cursor.swiftUIColor.opacity(max(0.45, opacity))
        }
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

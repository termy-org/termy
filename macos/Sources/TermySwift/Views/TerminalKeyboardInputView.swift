import AppKit
import SwiftUI

struct TerminalKeyboardInputView: NSViewRepresentable {
    var cols: Int
    var rows: Int
    var renderConfig: TerminalRenderConfig
    var isFocused: Bool
    var isInputEnabled: Bool
    var onFocus: () -> Void
    var onBytes: ([UInt8]) -> Void
    var onKeyInput: (TerminalKeyInput) -> Void
    var onScrollLines: (Int) -> Void
    var onScrollToTop: () -> Void
    var onScrollToBottom: () -> Void
    var onClearBuffer: () -> Void
    var onSplitRight: () -> Void
    var onSplitDown: () -> Void
    var onClosePane: () -> Void
    var onClosePaneIfSplit: () -> Bool
    var onFocusNextPane: () -> Void
    var onShowSearch: () -> Void
    var onSelectionChanged: (TerminalSelection?) -> Void
    var onCopy: () -> Bool

    func makeNSView(context: Context) -> KeyboardCaptureView {
        let view = KeyboardCaptureView()
        view.apply(configuration: self)
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor.clear.cgColor
        return view
    }

    func updateNSView(_ view: KeyboardCaptureView, context: Context) {
        view.apply(configuration: self)
        if isFocused, isInputEnabled {
            view.focus()
        }
    }
}

final class KeyboardCaptureView: NSView {
    var cols = 0
    var rows = 0
    var renderConfig = TerminalRenderConfig.default
    var isTerminalFocused = false
    var isInputEnabled = true
    var onFocus: () -> Void = {}
    var onBytes: ([UInt8]) -> Void = { _ in }
    var onKeyInput: (TerminalKeyInput) -> Void = { _ in }
    var onScrollLines: (Int) -> Void = { _ in }
    var onScrollToTop: () -> Void = {}
    var onScrollToBottom: () -> Void = {}
    var onClearBuffer: () -> Void = {}
    var onSplitRight: () -> Void = {}
    var onSplitDown: () -> Void = {}
    var onClosePane: () -> Void = {}
    var onClosePaneIfSplit: () -> Bool = { false }
    var onFocusNextPane: () -> Void = {}
    var onShowSearch: () -> Void = {}
    var onSelectionChanged: (TerminalSelection?) -> Void = { _ in }
    var onCopy: () -> Bool = { false }

    private var selectionAnchor: TerminalGridPosition?
    private var didDragSelection = false
    private var preciseScrollRemainder: CGFloat = 0

    override var acceptsFirstResponder: Bool {
        true
    }

    override var canBecomeKeyView: Bool {
        true
    }

    override var isOpaque: Bool {
        false
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard isInputEnabled else {
            return nil
        }
        return bounds.contains(point) ? self : nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if isTerminalFocused, isInputEnabled {
            focus()
        }
    }

    override func mouseDown(with event: NSEvent) {
        guard isInputEnabled else {
            super.mouseDown(with: event)
            return
        }
        onFocus()
        focus()
        didDragSelection = false
        selectionAnchor = gridPosition(for: event)
        onSelectionChanged(nil)
    }

    override func mouseDragged(with event: NSEvent) {
        guard isInputEnabled else {
            super.mouseDragged(with: event)
            return
        }
        guard let anchor = selectionAnchor else {
            return
        }
        didDragSelection = true
        onSelectionChanged(TerminalSelection(anchor: anchor, active: gridPosition(for: event)))
    }

    override func mouseUp(with event: NSEvent) {
        guard isInputEnabled else {
            super.mouseUp(with: event)
            return
        }
        guard let anchor = selectionAnchor else {
            return
        }

        defer {
            selectionAnchor = nil
            didDragSelection = false
        }

        guard didDragSelection else {
            onSelectionChanged(nil)
            return
        }

        onSelectionChanged(TerminalSelection(anchor: anchor, active: gridPosition(for: event)))
    }

    override func keyDown(with event: NSEvent) {
        guard isInputEnabled else {
            super.keyDown(with: event)
            return
        }
        onFocus()

        if handleHostCommand(event) || handleCopy(event) || handlePaste(event) {
            return
        }

        if event.modifierFlags.contains(.command) {
            // cmd+up/down scroll the viewport to the history extremes.
            if handleCommandScroll(event) {
                return
            }
            // Forward macOS line-editing shortcuts (cmd+arrows/backspace/delete);
            // the core maps these to ^A/^E/^U/^K. Everything else falls through
            // to the menu/responder chain.
            if let keyCode = MacKeyCode(rawValue: event.keyCode),
               isLineEditingKeyCode(keyCode),
               let keyInput = terminalKeyInput(for: event) {
                onKeyInput(keyInput)
                return
            }
            super.keyDown(with: event)
            return
        }

        if let keyInput = terminalKeyInput(for: event) {
            onKeyInput(keyInput)
            return
        }

        super.keyDown(with: event)
    }

    private func isLineEditingKeyCode(_ keyCode: MacKeyCode) -> Bool {
        switch keyCode {
        case .deleteBackward, .forwardDelete, .leftArrow, .rightArrow, .home, .end:
            return true
        default:
            return false
        }
    }

    private func handleCommandScroll(_ event: NSEvent) -> Bool {
        guard let keyCode = MacKeyCode(rawValue: event.keyCode) else {
            return false
        }

        switch keyCode {
        case .upArrow:
            onScrollToTop()
            return true
        case .downArrow:
            onScrollToBottom()
            return true
        default:
            return false
        }
    }

    override func scrollWheel(with event: NSEvent) {
        guard isInputEnabled else {
            super.scrollWheel(with: event)
            return
        }
        onFocus()

        let deltaLines: Int
        if event.hasPreciseScrollingDeltas {
            let rawLines = (event.scrollingDeltaY / max(1, renderConfig.cellHeight))
                + preciseScrollRemainder
            let wholeLines = rawLines.rounded(.towardZero)
            preciseScrollRemainder = rawLines - wholeLines
            deltaLines = Int(wholeLines)
        } else {
            deltaLines = Int(event.scrollingDeltaY.rounded())
        }

        if deltaLines != 0 {
            onScrollLines(deltaLines)
        }
    }

    func focus() {
        guard let window, window.firstResponder !== self else {
            return
        }
        window.makeFirstResponder(self)
    }

    private func handleCopy(_ event: NSEvent) -> Bool {
        guard event.modifierFlags.contains(.command),
              event.charactersIgnoringModifiers?.lowercased() == "c"
        else {
            return false
        }
        return onCopy()
    }

    private func handlePaste(_ event: NSEvent) -> Bool {
        guard event.modifierFlags.contains(.command),
              event.charactersIgnoringModifiers?.lowercased() == "v",
              let text = NSPasteboard.general.string(forType: .string)
        else {
            return false
        }

        onBytes(Array(text.utf8))
        return true
    }

    private func handleHostCommand(_ event: NSEvent) -> Bool {
        guard event.modifierFlags.contains(.command),
              let key = event.charactersIgnoringModifiers?.lowercased()
        else {
            return false
        }

        switch (key, event.modifierFlags.contains(.shift)) {
        case ("d", false):
            onSplitRight()
            return true
        case ("d", true):
            onSplitDown()
            return true
        case ("w", true):
            onClosePane()
            return true
        case ("w", false):
            if onClosePaneIfSplit() {
                return true
            }
            return false
        case ("]", _):
            onFocusNextPane()
            return true
        case ("f", _):
            onShowSearch()
            return true
        case ("k", false):
            onClearBuffer()
            return true
        default:
            return false
        }
    }

    private func terminalKeyInput(for event: NSEvent) -> TerminalKeyInput? {
        guard let keyCode = MacKeyCode(rawValue: event.keyCode) else {
            return characterKeyInput(for: event)
        }

        switch keyCode {
        case .returnKey, .keypadEnter:
            return keyInput("enter", event: event)
        case .tab:
            return keyInput("tab", event: event)
        case .deleteBackward:
            return keyInput("backspace", event: event)
        case .escape:
            return keyInput("escape", event: event)
        case .home:
            return keyInput("home", event: event)
        case .end:
            return keyInput("end", event: event)
        case .pageUp:
            return keyInput("pageup", event: event)
        case .pageDown:
            return keyInput("pagedown", event: event)
        case .forwardDelete:
            return keyInput("delete", event: event)
        case .leftArrow:
            return keyInput("left", event: event)
        case .rightArrow:
            return keyInput("right", event: event)
        case .downArrow:
            return keyInput("down", event: event)
        case .upArrow:
            return keyInput("up", event: event)
        case .f1:
            return keyInput("f1", event: event, function: true)
        case .f2:
            return keyInput("f2", event: event, function: true)
        case .f3:
            return keyInput("f3", event: event, function: true)
        case .f4:
            return keyInput("f4", event: event, function: true)
        case .f5:
            return keyInput("f5", event: event, function: true)
        case .f6:
            return keyInput("f6", event: event, function: true)
        case .f7:
            return keyInput("f7", event: event, function: true)
        case .f8:
            return keyInput("f8", event: event, function: true)
        case .f9:
            return keyInput("f9", event: event, function: true)
        case .f10:
            return keyInput("f10", event: event, function: true)
        case .f11:
            return keyInput("f11", event: event, function: true)
        case .f12:
            return keyInput("f12", event: event, function: true)
        case .space:
            return keyInput("space", event: event, keyChar: event.characters)
        }
    }

    private func characterKeyInput(for event: NSEvent) -> TerminalKeyInput? {
        guard let key = event.charactersIgnoringModifiers, !key.isEmpty else {
            return nil
        }
        return keyInput(
            key.lowercased(),
            event: event,
            keyChar: event.characters
        )
    }

    private func keyInput(
        _ key: String,
        event: NSEvent,
        keyChar: String? = nil,
        function: Bool = false
    ) -> TerminalKeyInput {
        let flags = event.modifierFlags
        return TerminalKeyInput(
            key: key,
            keyChar: keyChar,
            control: flags.contains(.control),
            alt: flags.contains(.option),
            shift: flags.contains(.shift),
            platform: flags.contains(.command),
            function: flags.contains(.function) || function,
            eventKind: event.isARepeat ? .repeat : .press
        )
    }

    private func gridPosition(for event: NSEvent) -> TerminalGridPosition {
        let point = convert(event.locationInWindow, from: nil)
        let maxCol = max(0, cols - 1)
        let maxRow = max(0, rows - 1)
        let col = max(0, min(Int((point.x - renderConfig.paddingX) / renderConfig.cellWidth), maxCol))
        let topY = bounds.height - point.y - renderConfig.paddingY
        let rowFromTop = Int(topY / renderConfig.cellHeight)
        let row = max(0, min(rowFromTop, maxRow))
        return TerminalGridPosition(col: col, row: row)
    }

}

private enum MacKeyCode: UInt16 {
    case returnKey = 36
    case keypadEnter = 76
    case tab = 48
    case deleteBackward = 51
    case escape = 53
    case home = 115
    case end = 119
    case pageUp = 116
    case pageDown = 121
    case forwardDelete = 117
    case leftArrow = 123
    case rightArrow = 124
    case downArrow = 125
    case upArrow = 126
    case f1 = 122
    case f2 = 120
    case f3 = 99
    case f4 = 118
    case f5 = 96
    case f6 = 97
    case f7 = 98
    case f8 = 100
    case f9 = 101
    case f10 = 109
    case f11 = 103
    case f12 = 111
    case space = 49
}

private extension KeyboardCaptureView {
    func apply(configuration: TerminalKeyboardInputView) {
        cols = configuration.cols
        rows = configuration.rows
        renderConfig = configuration.renderConfig
        isTerminalFocused = configuration.isFocused
        isInputEnabled = configuration.isInputEnabled
        onFocus = configuration.onFocus
        onBytes = configuration.onBytes
        onKeyInput = configuration.onKeyInput
        onScrollLines = configuration.onScrollLines
        onScrollToTop = configuration.onScrollToTop
        onScrollToBottom = configuration.onScrollToBottom
        onClearBuffer = configuration.onClearBuffer
        onSplitRight = configuration.onSplitRight
        onSplitDown = configuration.onSplitDown
        onClosePane = configuration.onClosePane
        onClosePaneIfSplit = configuration.onClosePaneIfSplit
        onFocusNextPane = configuration.onFocusNextPane
        onShowSearch = configuration.onShowSearch
        onSelectionChanged = configuration.onSelectionChanged
        onCopy = configuration.onCopy
    }
}

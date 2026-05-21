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
    var onSplitRight: () -> Void
    var onSplitDown: () -> Void
    var onClosePane: () -> Void
    var onFocusNextPane: () -> Void
    var onShowSearch: () -> Void
    var onSelectionChanged: (TerminalSelection?) -> Void
    var onCopy: () -> Bool

    func makeNSView(context: Context) -> KeyboardCaptureView {
        let view = KeyboardCaptureView()
        view.cols = cols
        view.rows = rows
        view.renderConfig = renderConfig
        view.isTerminalFocused = isFocused
        view.isInputEnabled = isInputEnabled
        view.onFocus = onFocus
        view.onBytes = onBytes
        view.onKeyInput = onKeyInput
        view.onScrollLines = onScrollLines
        view.onSplitRight = onSplitRight
        view.onSplitDown = onSplitDown
        view.onClosePane = onClosePane
        view.onFocusNextPane = onFocusNextPane
        view.onShowSearch = onShowSearch
        view.onSelectionChanged = onSelectionChanged
        view.onCopy = onCopy
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor.clear.cgColor
        return view
    }

    func updateNSView(_ view: KeyboardCaptureView, context: Context) {
        view.cols = cols
        view.rows = rows
        view.renderConfig = renderConfig
        view.isTerminalFocused = isFocused
        view.isInputEnabled = isInputEnabled
        view.onFocus = onFocus
        view.onBytes = onBytes
        view.onKeyInput = onKeyInput
        view.onScrollLines = onScrollLines
        view.onSplitRight = onSplitRight
        view.onSplitDown = onSplitDown
        view.onClosePane = onClosePane
        view.onFocusNextPane = onFocusNextPane
        view.onShowSearch = onShowSearch
        view.onSelectionChanged = onSelectionChanged
        view.onCopy = onCopy
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
    var onSplitRight: () -> Void = {}
    var onSplitDown: () -> Void = {}
    var onClosePane: () -> Void = {}
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
            super.keyDown(with: event)
            return
        }

        if let keyInput = terminalKeyInput(for: event) {
            onKeyInput(keyInput)
            return
        }

        super.keyDown(with: event)
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
            if TerminalCommandRouter.shared.closeFocusedPaneIfSplit() {
                return true
            }
            return false
        case ("]", _):
            onFocusNextPane()
            return true
        case ("f", _):
            onShowSearch()
            return true
        default:
            return false
        }
    }

    private func terminalKeyInput(for event: NSEvent) -> TerminalKeyInput? {
        switch event.keyCode {
        case 36, 76:
            return keyInput("enter", event: event)
        case 48:
            return keyInput("tab", event: event)
        case 51:
            return keyInput("backspace", event: event)
        case 53:
            return keyInput("escape", event: event)
        case 115:
            return keyInput("home", event: event)
        case 119:
            return keyInput("end", event: event)
        case 116:
            return keyInput("pageup", event: event)
        case 121:
            return keyInput("pagedown", event: event)
        case 117:
            return keyInput("delete", event: event)
        case 123:
            return keyInput("left", event: event)
        case 124:
            return keyInput("right", event: event)
        case 125:
            return keyInput("down", event: event)
        case 126:
            return keyInput("up", event: event)
        case 122:
            return keyInput("f1", event: event, function: true)
        case 120:
            return keyInput("f2", event: event, function: true)
        case 99:
            return keyInput("f3", event: event, function: true)
        case 118:
            return keyInput("f4", event: event, function: true)
        case 96:
            return keyInput("f5", event: event, function: true)
        case 97:
            return keyInput("f6", event: event, function: true)
        case 98:
            return keyInput("f7", event: event, function: true)
        case 100:
            return keyInput("f8", event: event, function: true)
        case 101:
            return keyInput("f9", event: event, function: true)
        case 109:
            return keyInput("f10", event: event, function: true)
        case 103:
            return keyInput("f11", event: event, function: true)
        case 111:
            return keyInput("f12", event: event, function: true)
        case 49:
            return keyInput("space", event: event, keyChar: event.characters)
        default:
            break
        }

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
            eventKind: event.isARepeat ? 2 : 1
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

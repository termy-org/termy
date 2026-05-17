import AppKit
import SwiftUI

struct TerminalKeyboardInputView: NSViewRepresentable {
    var cols: Int
    var rows: Int
    var renderConfig: TerminalRenderConfig
    var onBytes: ([UInt8]) -> Void
    var onSelectionChanged: (TerminalSelection?) -> Void
    var onCopy: () -> Bool

    func makeNSView(context: Context) -> KeyboardCaptureView {
        let view = KeyboardCaptureView()
        view.cols = cols
        view.rows = rows
        view.renderConfig = renderConfig
        view.onBytes = onBytes
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
        view.onBytes = onBytes
        view.onSelectionChanged = onSelectionChanged
        view.onCopy = onCopy
        view.focus()
    }
}

final class KeyboardCaptureView: NSView {
    var cols = 0
    var rows = 0
    var renderConfig = TerminalRenderConfig.default
    var onBytes: ([UInt8]) -> Void = { _ in }
    var onSelectionChanged: (TerminalSelection?) -> Void = { _ in }
    var onCopy: () -> Bool = { false }

    private var selectionAnchor: TerminalGridPosition?
    private var didDragSelection = false

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
        bounds.contains(point) ? self : nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        focus()
    }

    override func mouseDown(with event: NSEvent) {
        focus()
        didDragSelection = false
        selectionAnchor = gridPosition(for: event)
        onSelectionChanged(nil)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let anchor = selectionAnchor else {
            return
        }
        didDragSelection = true
        onSelectionChanged(TerminalSelection(anchor: anchor, active: gridPosition(for: event)))
    }

    override func mouseUp(with event: NSEvent) {
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
        if handleCopy(event) || handlePaste(event) {
            return
        }

        if event.modifierFlags.contains(.command) {
            super.keyDown(with: event)
            return
        }

        if let bytes = terminalBytes(for: event) {
            onBytes(bytes)
            return
        }

        super.keyDown(with: event)
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

    private func terminalBytes(for event: NSEvent) -> [UInt8]? {
        if let controlBytes = controlBytes(for: event) {
            return controlBytes
        }

        switch event.keyCode {
        case 36, 76:
            return [13]
        case 48:
            return [9]
        case 51:
            return [127]
        case 53:
            return [27]
        case 115:
            return escape("[H")
        case 119:
            return escape("[F")
        case 116:
            return escape("[5~")
        case 121:
            return escape("[6~")
        case 117:
            return escape("[3~")
        case 123:
            return escape("[D")
        case 124:
            return escape("[C")
        case 125:
            return escape("[B")
        case 126:
            return escape("[A")
        default:
            break
        }

        guard let characters = event.characters, !characters.isEmpty else {
            return nil
        }
        return Array(characters.utf8)
    }

    private func controlBytes(for event: NSEvent) -> [UInt8]? {
        guard event.modifierFlags.contains(.control),
              let character = event.charactersIgnoringModifiers?.lowercased().unicodeScalars.first
        else {
            return nil
        }

        switch character.value {
        case 64:
            return [0]
        case 65...90:
            return [UInt8(character.value - 64)]
        case 97...122:
            return [UInt8(character.value - 96)]
        case 91:
            return [27]
        case 92:
            return [28]
        case 93:
            return [29]
        case 94:
            return [30]
        case 95:
            return [31]
        default:
            return nil
        }
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

    private func escape(_ suffix: String) -> [UInt8] {
        Array("\u{1B}\(suffix)".utf8)
    }
}

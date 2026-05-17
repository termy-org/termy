import AppKit
import SwiftUI

struct TerminalKeyboardInputView: NSViewRepresentable {
    var onBytes: ([UInt8]) -> Void

    func makeNSView(context: Context) -> KeyboardCaptureView {
        let view = KeyboardCaptureView()
        view.onBytes = onBytes
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor.clear.cgColor
        return view
    }

    func updateNSView(_ view: KeyboardCaptureView, context: Context) {
        view.onBytes = onBytes
        view.focus()
    }
}

final class KeyboardCaptureView: NSView {
    var onBytes: ([UInt8]) -> Void = { _ in }

    override var acceptsFirstResponder: Bool {
        true
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        focus()
    }

    override func mouseDown(with event: NSEvent) {
        focus()
    }

    override func keyDown(with event: NSEvent) {
        if handlePaste(event) {
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
        guard window?.firstResponder !== self else {
            return
        }
        DispatchQueue.main.async { [weak self] in
            guard let self else {
                return
            }
            self.window?.makeFirstResponder(self)
        }
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

    private func escape(_ suffix: String) -> [UInt8] {
        Array("\u{1B}\(suffix)".utf8)
    }
}

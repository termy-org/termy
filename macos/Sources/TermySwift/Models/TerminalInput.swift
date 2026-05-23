import Foundation

struct TerminalKeyInput: Equatable {
    var key: String
    var keyChar: String?
    var control: Bool
    var alt: Bool
    var shift: Bool
    var platform: Bool
    var function: Bool
    var eventKind: TerminalKeyEventKind

    init(
        key: String,
        keyChar: String? = nil,
        control: Bool = false,
        alt: Bool = false,
        shift: Bool = false,
        platform: Bool = false,
        function: Bool = false,
        eventKind: TerminalKeyEventKind = .press
    ) {
        self.key = key
        self.keyChar = keyChar
        self.control = control
        self.alt = alt
        self.shift = shift
        self.platform = platform
        self.function = function
        self.eventKind = eventKind
    }
}

enum TerminalKeyEventKind: UInt32 {
    case press = 1
    case `repeat` = 2
    case release = 3
}

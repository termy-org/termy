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

struct TerminalSearchOptions: Equatable {
    var caseSensitive: Bool = false
    var usesRegex: Bool = false
}

struct TerminalMouseInput: Equatable {
    var kind: TerminalMouseEventKind
    var button: TerminalMouseButton
    var position: TerminalGridPosition
    var control: Bool
    var alt: Bool
    var shift: Bool
}

enum TerminalMouseEventKind: UInt32 {
    case press = 1
    case release = 2
    case drag = 3
    case move = 4
    case wheelUp = 5
    case wheelDown = 6
    case wheelLeft = 7
    case wheelRight = 8
}

enum TerminalMouseButton: UInt32 {
    case none = 0
    case left = 1
    case middle = 2
    case right = 3
}

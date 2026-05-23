import CTermy
import Foundation

enum TerminalRuntimeEventKind: UInt32 {
    case wakeup = 1
    case title = 2
    case resetTitle = 3
    case bell = 4
    case exit = 5
    case clipboardStore = 6
    case shellPromptStart = 7
    case shellCommandStart = 8
    case shellCommandExecuting = 9
    case shellCommandFinished = 10
    case progress = 11
    case workingDirectory = 12
}

enum TerminalProgressKind: UInt8 {
    case clear = 0
    case inProgress = 1
    case error = 2
    case indeterminate = 3
    case warning = 4
}

struct TerminalProgressPercent: Equatable {
    var value: UInt8

    init(_ value: UInt8) {
        self.value = min(value, 100)
    }

    var fraction: Double {
        Double(value) / 100.0
    }
}

enum TerminalProgress: Equatable {
    case clear
    case inProgress(TerminalProgressPercent)
    case error(TerminalProgressPercent)
    case indeterminate
    case warning(TerminalProgressPercent)

    init(state: UInt8, value: UInt8) {
        switch TerminalProgressKind(rawValue: state) {
        case .inProgress:
            self = .inProgress(TerminalProgressPercent(value))
        case .error:
            self = .error(TerminalProgressPercent(value))
        case .indeterminate:
            self = .indeterminate
        case .warning:
            self = .warning(TerminalProgressPercent(value))
        case .clear, nil:
            self = .clear
        }
    }

    var isVisible: Bool {
        self != .clear
    }

    var fraction: Double? {
        switch self {
        case let .inProgress(value), let .error(value), let .warning(value):
            return value.fraction
        case .clear, .indeterminate:
            return nil
        }
    }
}

enum TerminalRuntimeEvent: Equatable {
    case wakeup
    case title(String)
    case resetTitle
    case bell
    case exit
    case clipboardStore(String)
    case shellPromptStart
    case shellCommandStart
    case shellCommandExecuting
    case shellCommandFinished(Int32?)
    case progress(TerminalProgress)
    case workingDirectory(String)
}

struct TerminalSearchMatch: Identifiable, Equatable {
    var id: String {
        "\(row):\(startCol):\(endCol)"
    }

    var row: Int
    var startCol: Int
    var endCol: Int
}

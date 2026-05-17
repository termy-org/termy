import CTermy
import Foundation

enum TerminalProgress: Equatable {
    case clear
    case inProgress(UInt8)
    case error(UInt8)
    case indeterminate
    case warning(UInt8)

    init(state: UInt8, value: UInt8) {
        switch state {
        case UInt8(TERMY_FFI_PROGRESS_IN_PROGRESS.rawValue):
            self = .inProgress(value)
        case UInt8(TERMY_FFI_PROGRESS_ERROR.rawValue):
            self = .error(value)
        case UInt8(TERMY_FFI_PROGRESS_INDETERMINATE.rawValue):
            self = .indeterminate
        case UInt8(TERMY_FFI_PROGRESS_WARNING.rawValue):
            self = .warning(value)
        default:
            self = .clear
        }
    }

    var isVisible: Bool {
        self != .clear
    }

    var fraction: Double? {
        switch self {
        case let .inProgress(value), let .error(value), let .warning(value):
            return Double(value) / 100.0
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
    var line: String
}

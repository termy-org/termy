import SwiftUI

private struct TerminalCommandsFocusedValueKey: FocusedValueKey {
    typealias Value = TerminalCommandSet
}

extension FocusedValues {
    var terminalCommands: TerminalCommandSet? {
        get { self[TerminalCommandsFocusedValueKey.self] }
        set { self[TerminalCommandsFocusedValueKey.self] = newValue }
    }
}

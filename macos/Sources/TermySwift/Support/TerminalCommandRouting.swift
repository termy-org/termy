import Foundation

@MainActor
final class TerminalCommandRouter {
    static let shared = TerminalCommandRouter()

    weak var activeStore: TerminalWorkspaceStore?

    func closeFocusedPaneIfSplit() -> Bool {
        activeStore?.closeFocusedPaneIfSplit() ?? false
    }
}

struct TerminalCommandSet {
    var splitRight: () -> Void
    var splitDown: () -> Void
    var closePane: () -> Void
    var focusNextPane: () -> Void
    var showSearch: () -> Void
    var hideSearch: () -> Void
    var sendInterrupt: () -> Void
}

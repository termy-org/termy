import Foundation

struct TerminalFrameLink: Equatable, Identifiable {
    var id: String {
        "\(row):\(startCol):\(endCol):\(target)"
    }

    var row: Int
    var startCol: Int
    var endCol: Int
    var target: String
}

extension TerminalFrame {
    func link(at position: TerminalGridPosition) -> TerminalFrameLink? {
        let _ = position
        // Intentionally empty until libtermy/FFI exposes canonical link spans.
        // The GPUI app detects links in the terminal layer; Swift should consume
        // the same metadata instead of parsing rendered text independently.
        return nil
    }
}

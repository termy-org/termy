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

/// Detects clickable URLs in a line of terminal text using the system data
/// detector, restricted to web-style schemes so ordinary words aren't matched.
private final class TerminalLinkDetector: @unchecked Sendable {
    static let shared = TerminalLinkDetector()

    private let detector = try? NSDataDetector(
        types: NSTextCheckingResult.CheckingType.link.rawValue
    )
    private static let allowedSchemes: Set<String> = ["http", "https", "ftp", "mailto", "file"]

    func matches(in line: String) -> [(range: NSRange, target: String)] {
        guard let detector, !line.isEmpty else {
            return []
        }
        let range = NSRange(line.startIndex..<line.endIndex, in: line)
        return detector.matches(in: line, options: [], range: range).compactMap { result in
            guard let url = result.url,
                  let scheme = url.scheme?.lowercased(),
                  Self.allowedSchemes.contains(scheme)
            else {
                return nil
            }
            return (result.range, url.absoluteString)
        }
    }
}

extension TerminalFrame {
    /// The link under `position`, if any, in viewport coordinates.
    func link(at position: TerminalGridPosition) -> TerminalFrameLink? {
        guard position.row >= 0, position.row < rows,
              position.col >= 0, position.col < cols
        else {
            return nil
        }
        return links(inRow: position.row).first {
            position.col >= $0.startCol && position.col <= $0.endCol
        }
    }

    /// All links in a viewport row. Each cell maps to one column, so the data
    /// detector's character ranges map 1:1 onto grid columns.
    func links(inRow row: Int) -> [TerminalFrameLink] {
        guard cols > 0, row >= 0, row < rows else {
            return []
        }
        let characters = (0..<cols).map { col -> Character in
            guard let cell = cell(row: row, col: col), cell.renderText else {
                return " "
            }
            return cell.character
        }
        let line = String(characters)

        return TerminalLinkDetector.shared.matches(in: line).compactMap { match in
            guard let range = Range(match.range, in: line) else {
                return nil
            }
            let startCol = line.distance(from: line.startIndex, to: range.lowerBound)
            let endCol = line.distance(from: line.startIndex, to: range.upperBound) - 1
            guard startCol >= 0, endCol >= startCol, endCol < cols else {
                return nil
            }
            return TerminalFrameLink(
                row: row,
                startCol: startCol,
                endCol: endCol,
                target: match.target
            )
        }
    }
}

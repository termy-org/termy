import CTermy
import SwiftUI

struct TerminalRGBA: Equatable {
    var red: Double
    var green: Double
    var blue: Double
    var alpha: Double

    init(_ color: TermyFfiColor) {
        red = Double(color.r) / 255.0
        green = Double(color.g) / 255.0
        blue = Double(color.b) / 255.0
        alpha = Double(color.a) / 255.0
    }

    var swiftUIColor: Color {
        Color(red: red, green: green, blue: blue, opacity: alpha)
    }
}

struct TerminalCell: Identifiable, Equatable {
    var id: Int {
        (row * 10_000) + col
    }

    var col: Int
    var row: Int
    var character: Character
    var foreground: TerminalRGBA
    var background: TerminalRGBA
    var renderText: Bool
    var bold: Bool
}

struct TerminalCursor: Equatable {
    var col: Int
    var row: Int
    var style: UInt32
}

struct TerminalFrame: Equatable {
    var cols: Int
    var rows: Int
    var cells: [TerminalCell]
    var cursor: TerminalCursor?

    static let empty = TerminalFrame(cols: 0, rows: 0, cells: [], cursor: nil)

    func cell(row: Int, col: Int) -> TerminalCell? {
        let index = (row * cols) + col
        guard index >= 0, index < cells.count else {
            return nil
        }
        return cells[index]
    }
}

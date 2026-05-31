import CTermy
import SwiftUI

struct TerminalRGBA: Equatable {
    var red: Double
    var green: Double
    var blue: Double
    var alpha: Double

    init(red: Double, green: Double, blue: Double, alpha: Double) {
        self.red = red
        self.green = green
        self.blue = blue
        self.alpha = alpha
    }

    init(_ color: TermyFfiColor) {
        red = Double(color.r) / 255.0
        green = Double(color.g) / 255.0
        blue = Double(color.b) / 255.0
        alpha = Double(color.a) / 255.0
    }

    var swiftUIColor: Color {
        Color(red: red, green: green, blue: blue, opacity: alpha)
    }

    static let termyForeground = TerminalRGBA(red: 0.91, green: 0.92, blue: 0.96, alpha: 1.0)
    static let termyBackground = TerminalRGBA(red: 0.04, green: 0.06, blue: 0.13, alpha: 1.0)
    static let termyCursor = TerminalRGBA(red: 0.65, green: 0.91, blue: 0.64, alpha: 1.0)
}

struct TerminalRenderConfig: Equatable {
    var fontFamily: String
    var activeTheme: String
    var foreground: TerminalRGBA
    var background: TerminalRGBA
    var cursor: TerminalRGBA
    var fontSize: CGFloat
    var lineHeight: CGFloat
    var paddingX: CGFloat
    var paddingY: CGFloat
    var backgroundOpacity: Double
    var backgroundOpacityCells: Bool
    var cursorBlink: Bool
    var cursorStyle: TerminalCursorStyle
    var measuredCellWidth: CGFloat
    var measuredCellHeight: CGFloat
    var backgroundBlur: Bool
    var mouseScrollMultiplier: CGFloat
    var scrollbarVisibility: TerminalScrollbarVisibility
    var scrollbarStyle: TerminalScrollbarStyle
    var copyOnSelect: Bool
    var copyOnSelectToast: Bool
    var paneFocusEffect: TerminalPaneFocusEffect
    var paneFocusStrength: CGFloat
    var chromeContrast: Bool

    static let `default` = TerminalRenderConfig(
        fontFamily: "JetBrains Mono",
        activeTheme: "termy",
        foreground: .termyForeground,
        background: .termyBackground,
        cursor: .termyCursor,
        fontSize: 14.0,
        lineHeight: 1.4,
        paddingX: 12.0,
        paddingY: 8.0,
        backgroundOpacity: 1.0,
        backgroundOpacityCells: false,
        cursorBlink: true,
        cursorStyle: .block,
        measuredCellWidth: 9.0,
        measuredCellHeight: 19.6,
        backgroundBlur: false,
        mouseScrollMultiplier: 3.0,
        scrollbarVisibility: .onScroll,
        scrollbarStyle: .neutral,
        copyOnSelect: false,
        copyOnSelectToast: true,
        paneFocusEffect: .softSpotlight,
        paneFocusStrength: 0.6,
        chromeContrast: false
    )

    init(
        fontFamily: String,
        activeTheme: String,
        foreground: TerminalRGBA,
        background: TerminalRGBA,
        cursor: TerminalRGBA,
        fontSize: CGFloat,
        lineHeight: CGFloat,
        paddingX: CGFloat,
        paddingY: CGFloat,
        backgroundOpacity: Double,
        backgroundOpacityCells: Bool,
        cursorBlink: Bool,
        cursorStyle: TerminalCursorStyle,
        measuredCellWidth: CGFloat,
        measuredCellHeight: CGFloat,
        backgroundBlur: Bool,
        mouseScrollMultiplier: CGFloat,
        scrollbarVisibility: TerminalScrollbarVisibility,
        scrollbarStyle: TerminalScrollbarStyle,
        copyOnSelect: Bool,
        copyOnSelectToast: Bool,
        paneFocusEffect: TerminalPaneFocusEffect,
        paneFocusStrength: CGFloat,
        chromeContrast: Bool
    ) {
        self.fontFamily = fontFamily
        self.activeTheme = activeTheme
        self.foreground = foreground
        self.background = background
        self.cursor = cursor
        self.fontSize = max(1.0, fontSize)
        self.lineHeight = max(0.8, lineHeight)
        self.paddingX = max(0.0, paddingX)
        self.paddingY = max(0.0, paddingY)
        self.backgroundOpacity = min(1.0, max(0.0, backgroundOpacity))
        self.backgroundOpacityCells = backgroundOpacityCells
        self.cursorBlink = cursorBlink
        self.cursorStyle = cursorStyle
        self.measuredCellWidth = max(1.0, measuredCellWidth)
        self.measuredCellHeight = max(1.0, measuredCellHeight)
        self.backgroundBlur = backgroundBlur
        self.mouseScrollMultiplier = max(0.0, mouseScrollMultiplier)
        self.scrollbarVisibility = scrollbarVisibility
        self.scrollbarStyle = scrollbarStyle
        self.copyOnSelect = copyOnSelect
        self.copyOnSelectToast = copyOnSelectToast
        self.paneFocusEffect = paneFocusEffect
        self.paneFocusStrength = max(0.0, min(2.0, paneFocusStrength))
        self.chromeContrast = chromeContrast
    }

    init(_ ffiConfig: TermyFfiRenderConfig) {
        self.init(
            fontFamily: TermyFfiBridge.string(from: ffiConfig.font_family) ?? Self.default.fontFamily,
            activeTheme: TermyFfiBridge.string(from: ffiConfig.active_theme) ?? Self.default.activeTheme,
            foreground: TerminalRGBA(ffiConfig.foreground),
            background: TerminalRGBA(ffiConfig.background),
            cursor: TerminalRGBA(ffiConfig.cursor),
            fontSize: CGFloat(ffiConfig.font_size),
            lineHeight: CGFloat(ffiConfig.line_height),
            paddingX: CGFloat(ffiConfig.padding_x),
            paddingY: CGFloat(ffiConfig.padding_y),
            backgroundOpacity: Double(ffiConfig.background_opacity),
            backgroundOpacityCells: ffiConfig.background_opacity_cells,
            cursorBlink: ffiConfig.cursor_blink,
            cursorStyle: TerminalCursorStyle(ffiRawValue: ffiConfig.cursor_style),
            measuredCellWidth: CGFloat(ffiConfig.cell_width),
            measuredCellHeight: CGFloat(ffiConfig.cell_height),
            backgroundBlur: ffiConfig.background_blur,
            mouseScrollMultiplier: CGFloat(ffiConfig.mouse_scroll_multiplier),
            scrollbarVisibility: TerminalScrollbarVisibility(ffiRawValue: ffiConfig.scrollbar_visibility),
            scrollbarStyle: TerminalScrollbarStyle(ffiRawValue: ffiConfig.scrollbar_style),
            copyOnSelect: ffiConfig.copy_on_select,
            copyOnSelectToast: ffiConfig.copy_on_select_toast,
            paneFocusEffect: TerminalPaneFocusEffect(ffiRawValue: ffiConfig.pane_focus_effect),
            paneFocusStrength: CGFloat(ffiConfig.pane_focus_strength),
            chromeContrast: ffiConfig.chrome_contrast
        )
    }

    var cellWidth: CGFloat {
        measuredCellWidth
    }

    var cellHeight: CGFloat {
        measuredCellHeight
    }
}

enum TerminalCursorStyle: UInt32 {
    case line = 1
    case block = 2

    init(ffiRawValue: UInt32) {
        self = TerminalCursorStyle(rawValue: ffiRawValue) ?? .block
    }
}

enum TerminalScrollbarVisibility: UInt32 {
    case off = 0
    case always = 1
    case onScroll = 2

    init(ffiRawValue: UInt32) {
        self = TerminalScrollbarVisibility(rawValue: ffiRawValue) ?? .onScroll
    }
}

enum TerminalScrollbarStyle: UInt32 {
    case neutral = 0
    case mutedTheme = 1
    case theme = 2

    init(ffiRawValue: UInt32) {
        self = TerminalScrollbarStyle(rawValue: ffiRawValue) ?? .neutral
    }
}

enum TerminalPaneFocusEffect: UInt32 {
    case off = 0
    case softSpotlight = 1
    case cinematic = 2
    case minimal = 3

    init(ffiRawValue: UInt32) {
        self = TerminalPaneFocusEffect(rawValue: ffiRawValue) ?? .softSpotlight
    }
}

struct TerminalGridPosition: Equatable {
    var col: Int
    var row: Int
}

struct TerminalSelection: Equatable {
    var anchor: TerminalGridPosition
    var active: TerminalGridPosition

    var normalized: (start: TerminalGridPosition, end: TerminalGridPosition) {
        if (anchor.row, anchor.col) <= (active.row, active.col) {
            return (anchor, active)
        }
        return (active, anchor)
    }

    func rowRanges(cols: Int, rows: Int) -> [TerminalSelectionRowRange] {
        guard cols > 0, rows > 0 else {
            return []
        }

        let range = normalized
        let startRow = max(0, min(range.start.row, rows - 1))
        let endRow = max(0, min(range.end.row, rows - 1))
        guard startRow <= endRow else {
            return []
        }

        return (startRow...endRow).map { row in
            let startCol = row == startRow ? range.start.col : 0
            let endCol = row == endRow ? range.end.col : cols - 1
            return TerminalSelectionRowRange(
                row: row,
                startCol: max(0, min(startCol, cols - 1)),
                endCol: max(0, min(endCol, cols - 1))
            )
        }
    }
}

struct TerminalSelectionRowRange: Identifiable, Equatable {
    var id: Int {
        row
    }

    var row: Int
    var startCol: Int
    var endCol: Int
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
    var usesTerminalDefaultBackground: Bool
    var renderText: Bool
    var bold: Bool
}

struct TerminalCursor: Equatable {
    var col: Int
    var row: Int
    var style: TerminalCursorStyle
}

struct TerminalFrame: Equatable {
    var cols: Int
    var rows: Int
    var cells: [TerminalCell]
    var cursor: TerminalCursor?
    var displayOffset: Int
    var historySize: Int

    static let empty = TerminalFrame(
        cols: 0,
        rows: 0,
        cells: [],
        cursor: nil,
        displayOffset: 0,
        historySize: 0
    )

    static func plainTextPreview(_ text: String, cols: Int = 96, rows: Int = 28) -> TerminalFrame {
        let cols = max(2, cols)
        let rows = max(2, rows)
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
            .suffix(rows)
            .map(String.init)
        let topPadding = max(0, rows - lines.count)
        var cells: [TerminalCell] = []
        cells.reserveCapacity(cols * rows)

        for row in 0..<rows {
            let lineIndex = row - topPadding
            let characters = lineIndex >= 0 ? Array(lines[lineIndex].prefix(cols)) : []
            for col in 0..<cols {
                let character = characters.indices.contains(col) ? characters[col] : " "
                cells.append(TerminalCell(
                    col: col,
                    row: row,
                    character: character,
                    foreground: .termyForeground,
                    background: .termyBackground,
                    usesTerminalDefaultBackground: true,
                    renderText: character != " ",
                    bold: false
                ))
            }
        }

        return TerminalFrame(
            cols: cols,
            rows: rows,
            cells: cells,
            cursor: nil,
            displayOffset: 0,
            historySize: 0
        )
    }

    func cells(inRow row: Int) -> ArraySlice<TerminalCell> {
        let start = row * cols
        let end = start + cols
        guard row >= 0, cols > 0, start >= 0, end <= cells.count else {
            return []
        }
        return cells[start..<end]
    }

    func cell(row: Int, col: Int) -> TerminalCell? {
        guard row >= 0, row < rows, col >= 0, col < cols else {
            return nil
        }
        let index = (row * cols) + col
        guard index >= 0, index < cells.count else {
            return nil
        }
        return cells[index]
    }

    func selectedText(for selection: TerminalSelection?) -> String? {
        guard let selection else {
            return nil
        }

        let lines = selection.rowRanges(cols: cols, rows: rows).map { range in
            let characters = (range.startCol...range.endCol).map { col -> Character in
                guard let cell = cell(row: range.row, col: col), cell.renderText else {
                    return " "
                }
                return cell.character
            }
            return String(characters).trimmingTrailingSpaces()
        }
        guard !lines.isEmpty else {
            return nil
        }
        return lines.joined(separator: "\n")
    }

    func visibleTextSnapshot() -> String {
        guard cols > 0, rows > 0 else {
            return ""
        }
        return (0..<rows)
            .map { row in
                String(cells(inRow: row).map { $0.renderText ? $0.character : " " })
                    .trimmingTrailingSpaces()
            }
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Word selection for double-click: expands from `position` over a contiguous
    /// run of same-class characters (word vs. non-word punctuation), stopping at
    /// whitespace. Word characters include path/URL-friendly punctuation so file
    /// paths and URLs select as a single token. Returns nil over whitespace.
    func wordSelection(at position: TerminalGridPosition) -> TerminalSelection? {
        guard cols > 0, rows > 0,
              position.row >= 0, position.row < rows,
              position.col >= 0, position.col < cols
        else {
            return nil
        }

        func character(at col: Int) -> Character {
            guard let cell = cell(row: position.row, col: col), cell.renderText else {
                return " "
            }
            return cell.character
        }

        let target = character(at: position.col)
        guard !target.isWhitespace else {
            return nil
        }
        let targetIsWord = TerminalFrame.isWordCharacter(target)

        func matches(_ col: Int) -> Bool {
            let c = character(at: col)
            return !c.isWhitespace && TerminalFrame.isWordCharacter(c) == targetIsWord
        }

        var startCol = position.col
        while startCol > 0, matches(startCol - 1) {
            startCol -= 1
        }
        var endCol = position.col
        while endCol < cols - 1, matches(endCol + 1) {
            endCol += 1
        }

        return TerminalSelection(
            anchor: TerminalGridPosition(col: startCol, row: position.row),
            active: TerminalGridPosition(col: endCol, row: position.row)
        )
    }

    /// Line selection for triple-click: selects the full visible row. Trailing
    /// blanks are trimmed when the text is copied.
    func lineSelection(at position: TerminalGridPosition) -> TerminalSelection? {
        guard cols > 0, rows > 0, position.row >= 0, position.row < rows else {
            return nil
        }
        return TerminalSelection(
            anchor: TerminalGridPosition(col: 0, row: position.row),
            active: TerminalGridPosition(col: cols - 1, row: position.row)
        )
    }

    static func isWordCharacter(_ character: Character) -> Bool {
        if character.isLetter || character.isNumber {
            return true
        }
        return "_./-~:@%+=?&#".contains(character)
    }
}

private extension String {
    func trimmingTrailingSpaces() -> String {
        var result = self
        while result.last == " " {
            result.removeLast()
        }
        return result
    }
}

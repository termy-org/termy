import SwiftUI

struct TerminalGridView: View {
    let frame: TerminalFrame
    let selection: TerminalSelection?
    let renderConfig: TerminalRenderConfig
    let searchMatches: [TerminalSearchMatch]
    let activeSearchMatch: TerminalSearchMatch?

    var body: some View {
        ZStack(alignment: .topLeading) {
            backgroundOverlay
            searchOverlay
            selectionOverlay
            cursorOverlay

            textOverlay
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .clipped()
    }

    private var backgroundOverlay: some View {
        ZStack(alignment: .topLeading) {
            ForEach(frame.cells.filter(shouldPaintBackground)) { cell in
                Rectangle()
                    .fill(cell.background.swiftUIColor.opacity(backgroundOpacity(for: cell)))
                    .frame(width: renderConfig.cellWidth, height: renderConfig.cellHeight)
                    .offset(
                        x: renderConfig.paddingX + CGFloat(cell.col) * renderConfig.cellWidth,
                        y: renderConfig.paddingY + CGFloat(cell.row) * renderConfig.cellHeight
                    )
            }
        }
        .allowsHitTesting(false)
    }

    private func shouldPaintBackground(_ cell: TerminalCell) -> Bool {
        !cell.usesTerminalDefaultBackground || renderConfig.backgroundOpacityCells
    }

    private func backgroundOpacity(for cell: TerminalCell) -> Double {
        cell.usesTerminalDefaultBackground ? renderConfig.backgroundOpacity : 1.0
    }

    private var selectionOverlay: some View {
        ZStack(alignment: .topLeading) {
            ForEach(selection?.rowRanges(cols: frame.cols, rows: frame.rows) ?? []) { range in
                Rectangle()
                    .fill(Color.accentColor.opacity(0.35))
                    .frame(
                        width: CGFloat(range.endCol - range.startCol + 1)
                            * renderConfig.cellWidth,
                        height: renderConfig.cellHeight
                    )
                    .offset(
                        x: renderConfig.paddingX + CGFloat(range.startCol) * renderConfig.cellWidth,
                        y: renderConfig.paddingY + CGFloat(range.row) * renderConfig.cellHeight
                    )
            }
        }
        .allowsHitTesting(false)
    }

    private var searchOverlay: some View {
        ZStack(alignment: .topLeading) {
            ForEach(searchMatches) { match in
                Rectangle()
                    .fill(searchColor(for: match))
                    .frame(
                        width: CGFloat(max(1, match.endCol - match.startCol + 1))
                            * renderConfig.cellWidth,
                        height: renderConfig.cellHeight
                    )
                    .offset(
                        x: renderConfig.paddingX + CGFloat(match.startCol) * renderConfig.cellWidth,
                        y: renderConfig.paddingY + CGFloat(match.row) * renderConfig.cellHeight
                    )
            }
        }
        .allowsHitTesting(false)
    }

    private func searchColor(for match: TerminalSearchMatch) -> Color {
        if match == activeSearchMatch {
            return .orange.opacity(0.55)
        }
        return .yellow.opacity(0.28)
    }

    private var cursorOverlay: some View {
        Group {
            if let cursor = frame.cursor, frame.displayOffset == 0 {
                Rectangle()
                    .fill(renderConfig.cursor.swiftUIColor)
                    .frame(width: renderConfig.cellWidth, height: renderConfig.cellHeight)
                    .offset(
                        x: renderConfig.paddingX + CGFloat(cursor.col) * renderConfig.cellWidth,
                        y: renderConfig.paddingY + CGFloat(cursor.row) * renderConfig.cellHeight
                    )
            }
        }
        .allowsHitTesting(false)
    }

    private var textOverlay: some View {
        ZStack(alignment: .topLeading) {
            ForEach(textSegments) { segment in
                Text(verbatim: segment.text)
                    .font(terminalFont(weight: segment.bold ? .semibold : .regular))
                    .foregroundStyle(segment.foreground.swiftUIColor)
                    .lineLimit(1)
                    .frame(
                        width: CGFloat(segment.text.count) * renderConfig.cellWidth,
                        height: renderConfig.cellHeight,
                        alignment: .leading
                    )
                    .offset(
                        x: renderConfig.paddingX + CGFloat(segment.startCol) * renderConfig.cellWidth,
                        y: renderConfig.paddingY + CGFloat(segment.row) * renderConfig.cellHeight
                    )
            }
        }
        .allowsHitTesting(false)
    }

    private var textSegments: [TerminalTextSegment] {
        var segments: [TerminalTextSegment] = []
        for row in 0..<frame.rows {
            segments.append(contentsOf: textSegments(in: row))
        }
        return segments
    }

    private func textSegments(in row: Int) -> [TerminalTextSegment] {
        var segments: [TerminalTextSegment] = []
        var segment = ""
        var segmentForeground: TerminalRGBA?
        var segmentBold = false
        var segmentStartCol = 0

        func flush() {
            guard let foreground = segmentForeground, !segment.isEmpty else {
                return
            }

            segments.append(TerminalTextSegment(
                row: row,
                startCol: segmentStartCol,
                text: segment,
                foreground: foreground,
                bold: segmentBold
            ))
            segment = ""
        }

        for cell in frame.cells(inRow: row) {
            let foreground = cell.foreground
            let bold = cell.bold
            if segmentForeground != foreground || segmentBold != bold || !cell.renderText {
                flush()
                segmentForeground = foreground
                segmentBold = bold
                segmentStartCol = cell.col
            }
            if cell.renderText {
                segment.append(cell.character)
            }
        }

        flush()
        return segments
    }

    private func terminalFont(weight: Font.Weight) -> Font {
        let fontFamily = renderConfig.fontFamily.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !fontFamily.isEmpty else {
            return .system(size: renderConfig.fontSize, weight: weight, design: .monospaced)
        }
        return .custom(fontFamily, size: renderConfig.fontSize).weight(weight)
    }
}

private struct TerminalTextSegment: Identifiable {
    var row: Int
    var startCol: Int
    var text: String
    var foreground: TerminalRGBA
    var bold: Bool

    var id: String {
        "\(row):\(startCol):\(text.count):\(foreground.red):\(foreground.green):\(foreground.blue):\(bold)"
    }
}

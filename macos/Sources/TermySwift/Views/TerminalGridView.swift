import SwiftUI

struct TerminalGridView: View {
    let frame: TerminalFrame
    let selection: TerminalSelection?
    let renderConfig: TerminalRenderConfig
    let searchMatches: [TerminalSearchMatch]
    let activeSearchMatch: TerminalSearchMatch?

    var body: some View {
        Canvas(opaque: false, rendersAsynchronously: false) { context, _ in
            draw(in: &context)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .clipped()
    }

    private func draw(in context: inout GraphicsContext) {
        drawBackgrounds(in: &context)
        drawSearch(in: &context)
        drawSelection(in: &context)
        drawCursor(in: &context)
        drawText(in: &context)
    }

    private func cellRect(col: Int, row: Int, cols: Int = 1) -> CGRect {
        CGRect(
            x: renderConfig.paddingX + CGFloat(col) * renderConfig.cellWidth,
            y: renderConfig.paddingY + CGFloat(row) * renderConfig.cellHeight,
            width: CGFloat(cols) * renderConfig.cellWidth,
            height: renderConfig.cellHeight
        )
    }

    private func drawBackgrounds(in context: inout GraphicsContext) {
        for cell in frame.cells where shouldPaintBackground(cell) {
            let rect = cellRect(col: cell.col, row: cell.row)
            context.fill(
                Path(rect),
                with: .color(cell.background.swiftUIColor.opacity(backgroundOpacity(for: cell)))
            )
        }
    }

    private func shouldPaintBackground(_ cell: TerminalCell) -> Bool {
        !cell.usesTerminalDefaultBackground || renderConfig.backgroundOpacityCells
    }

    private func backgroundOpacity(for cell: TerminalCell) -> Double {
        cell.usesTerminalDefaultBackground ? renderConfig.backgroundOpacity : 1.0
    }

    private func drawSelection(in context: inout GraphicsContext) {
        guard let ranges = selection?.rowRanges(cols: frame.cols, rows: frame.rows) else {
            return
        }
        let fill = GraphicsContext.Shading.color(Color.accentColor.opacity(0.35))
        for range in ranges {
            let rect = cellRect(
                col: range.startCol,
                row: range.row,
                cols: range.endCol - range.startCol + 1
            )
            context.fill(Path(rect), with: fill)
        }
    }

    private func drawSearch(in context: inout GraphicsContext) {
        for match in searchMatches {
            let rect = cellRect(
                col: match.startCol,
                row: match.row,
                cols: max(1, match.endCol - match.startCol + 1)
            )
            context.fill(Path(rect), with: .color(searchColor(for: match)))
        }
    }

    private func searchColor(for match: TerminalSearchMatch) -> Color {
        if match == activeSearchMatch {
            return .orange.opacity(0.55)
        }
        return .yellow.opacity(0.28)
    }

    private func drawCursor(in context: inout GraphicsContext) {
        guard let cursor = frame.cursor, frame.displayOffset == 0 else {
            return
        }
        let rect = cellRect(col: cursor.col, row: cursor.row)
        context.fill(Path(rect), with: .color(renderConfig.cursor.swiftUIColor))
    }

    private func drawText(in context: inout GraphicsContext) {
        let centerY = renderConfig.cellHeight / 2
        for segment in textSegments {
            var text = Text(verbatim: segment.text)
                .font(terminalFont(weight: segment.bold ? .semibold : .regular))
            text = text.foregroundColor(segment.foreground.swiftUIColor)

            let resolved = context.resolve(text)
            let origin = CGPoint(
                x: renderConfig.paddingX + CGFloat(segment.startCol) * renderConfig.cellWidth,
                y: renderConfig.paddingY + CGFloat(segment.row) * renderConfig.cellHeight + centerY
            )
            context.draw(resolved, at: origin, anchor: .leading)
        }
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

private struct TerminalTextSegment {
    var row: Int
    var startCol: Int
    var text: String
    var foreground: TerminalRGBA
    var bold: Bool
}

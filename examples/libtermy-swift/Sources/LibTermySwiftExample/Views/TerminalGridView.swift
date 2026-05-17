import SwiftUI

struct TerminalGridView: View {
    let frame: TerminalFrame
    let selection: TerminalSelection?
    let renderConfig: TerminalRenderConfig

    var body: some View {
        ZStack(alignment: .topLeading) {
            selectionOverlay
            cursorOverlay

            VStack(alignment: .leading, spacing: 0) {
                ForEach(0..<frame.rows, id: \.self) { row in
                    rowText(row)
                        .font(terminalFont(weight: .regular))
                        .lineLimit(1)
                        .frame(
                            width: CGFloat(frame.cols) * renderConfig.cellWidth,
                            height: renderConfig.cellHeight,
                            alignment: .leading
                        )
                }
            }
            .offset(x: renderConfig.paddingX, y: renderConfig.paddingY)
            .allowsHitTesting(false)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .clipped()
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

    private var cursorOverlay: some View {
        Group {
            if let cursor = frame.cursor {
                Rectangle()
                    .fill(Color.primary.opacity(0.85))
                    .frame(width: renderConfig.cellWidth, height: renderConfig.cellHeight)
                    .offset(
                        x: renderConfig.paddingX + CGFloat(cursor.col) * renderConfig.cellWidth,
                        y: renderConfig.paddingY + CGFloat(cursor.row) * renderConfig.cellHeight
                    )
            }
        }
        .allowsHitTesting(false)
    }

    private func rowText(_ row: Int) -> Text {
        var result = Text(verbatim: "")
        var segment = ""
        var segmentForeground: TerminalRGBA?
        var segmentBold = false

        func flush() {
            guard let foreground = segmentForeground, !segment.isEmpty else {
                return
            }

            result = result + Text(verbatim: segment)
                .foregroundColor(foreground.swiftUIColor)
                .font(terminalFont(weight: segmentBold ? .semibold : .regular))
            segment = ""
        }

        for cell in frame.cells(inRow: row) {
            let foreground = cell.foreground
            let bold = cell.bold
            if segmentForeground != foreground || segmentBold != bold {
                flush()
                segmentForeground = foreground
                segmentBold = bold
            }
            segment.append(cell.renderText ? cell.character : " ")
        }

        flush()
        return result
    }

    private func terminalFont(weight: Font.Weight) -> Font {
        let fontFamily = renderConfig.fontFamily.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !fontFamily.isEmpty else {
            return .system(size: renderConfig.fontSize, weight: weight, design: .monospaced)
        }
        return .custom(fontFamily, size: renderConfig.fontSize).weight(weight)
    }
}

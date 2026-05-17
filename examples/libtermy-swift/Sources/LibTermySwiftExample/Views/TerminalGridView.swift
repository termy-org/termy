import SwiftUI

enum TerminalGridMetrics {
    static let cellWidth: CGFloat = 8.2
    static let cellHeight: CGFloat = 17.0
    static let fontSize: CGFloat = 13.0
}

struct TerminalGridView: View {
    let frame: TerminalFrame

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(0..<frame.rows, id: \.self) { row in
                HStack(spacing: 0) {
                    ForEach(0..<frame.cols, id: \.self) { col in
                        if let cell = frame.cell(row: row, col: col) {
                            TerminalCellView(
                                cell: cell,
                                isCursor: frame.cursor?.row == row && frame.cursor?.col == col
                            )
                            .frame(
                                width: TerminalGridMetrics.cellWidth,
                                height: TerminalGridMetrics.cellHeight
                            )
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

private struct TerminalCellView: View {
    let cell: TerminalCell
    let isCursor: Bool

    var body: some View {
        ZStack {
            cellBackground

            if cell.renderText {
                Text(String(cell.character))
                    .font(.system(
                        size: TerminalGridMetrics.fontSize,
                        weight: cell.bold ? .semibold : .regular,
                        design: .monospaced
                    ))
                    .foregroundStyle(isCursor ? Color.black : cell.foreground.swiftUIColor)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
            }
        }
    }

    private var cellBackground: some View {
        Rectangle()
            .fill(isCursor ? Color.primary : cell.background.swiftUIColor)
    }
}

import SwiftUI

struct TerminalGridView: View {
    let frame: TerminalFrame

    private let cellWidth: CGFloat = 8.2
    private let cellHeight: CGFloat = 17.0

    var body: some View {
        ScrollView([.horizontal, .vertical]) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(0..<frame.rows, id: \.self) { row in
                    HStack(spacing: 0) {
                        ForEach(0..<frame.cols, id: \.self) { col in
                            if let cell = frame.cell(row: row, col: col) {
                                TerminalCellView(
                                    cell: cell,
                                    isCursor: frame.cursor?.row == row && frame.cursor?.col == col
                                )
                                .frame(width: cellWidth, height: cellHeight)
                            }
                        }
                    }
                }
            }
            .padding(12)
        }
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
                    .font(.system(size: 13, weight: cell.bold ? .semibold : .regular, design: .monospaced))
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

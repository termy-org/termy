import SwiftUI

struct ContentView: View {
    @ObservedObject var terminal: TerminalViewModel

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .topLeading) {
                TerminalGridView(
                    frame: terminal.frame,
                    selection: terminal.selection,
                    renderConfig: terminal.renderConfig
                )
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

                TerminalKeyboardInputView(
                    cols: terminal.frame.cols,
                    rows: terminal.frame.rows,
                    renderConfig: terminal.renderConfig,
                    onBytes: { bytes in
                        terminal.send(bytes: bytes)
                    },
                    onSelectionChanged: { selection in
                        terminal.updateSelection(selection)
                    },
                    onCopy: {
                        terminal.copySelection()
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)

                if let errorMessage = terminal.errorMessage {
                    Text(errorMessage)
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(.red)
                        .padding(8)
                        .background(.regularMaterial)
                        .padding(8)
                }
            }
            .background(
                Color(red: 0.05, green: 0.055, blue: 0.06)
                    .opacity(terminal.renderConfig.backgroundOpacity)
            )
            .onAppear {
                terminal.start()
                resizeTerminal(to: proxy.size)
            }
            .onChange(of: proxy.size) { _, size in
                resizeTerminal(to: size)
            }
            .onDisappear {
                terminal.stop()
            }
        }
    }

    private func resizeTerminal(to size: CGSize) {
        terminal.resize(
            cols: Int((size.width - (terminal.renderConfig.paddingX * 2))
                / terminal.renderConfig.cellWidth),
            rows: Int((size.height - (terminal.renderConfig.paddingY * 2))
                / terminal.renderConfig.cellHeight),
            cellWidth: terminal.renderConfig.cellWidth,
            cellHeight: terminal.renderConfig.cellHeight
        )
    }
}

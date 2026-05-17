import SwiftUI

struct ContentView: View {
    @ObservedObject var terminal: TerminalViewModel

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .topLeading) {
                TerminalGridView(frame: terminal.frame)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

                TerminalKeyboardInputView { bytes in
                    terminal.send(bytes: bytes)
                }
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
            .background(Color(red: 0.05, green: 0.055, blue: 0.06))
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
            cols: Int(size.width / TerminalGridMetrics.cellWidth),
            rows: Int(size.height / TerminalGridMetrics.cellHeight),
            cellWidth: TerminalGridMetrics.cellWidth,
            cellHeight: TerminalGridMetrics.cellHeight
        )
    }
}

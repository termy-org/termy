import SwiftUI

struct ContentView: View {
    @ObservedObject var terminal: TerminalViewModel
    @FocusState private var commandFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            TerminalToolbar(terminal: terminal)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)

            Divider()

            TerminalGridView(frame: terminal.frame)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .background(Color(red: 0.05, green: 0.055, blue: 0.06))

            Divider()

            HStack(spacing: 8) {
                TextField("Command", text: $terminal.commandText)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13, design: .monospaced))
                    .focused($commandFocused)
                    .onSubmit(terminal.sendCommand)

                Button("Send", action: terminal.sendCommand)
                    .keyboardShortcut(.return, modifiers: .command)
            }
            .padding(10)
        }
        .onAppear {
            terminal.start()
            commandFocused = true
        }
        .onDisappear {
            terminal.stop()
        }
    }
}

private struct TerminalToolbar: View {
    @ObservedObject var terminal: TerminalViewModel

    var body: some View {
        HStack(spacing: 10) {
            Text("libtermy SwiftUI")
                .font(.headline)

            Text("\(terminal.frame.cols)x\(terminal.frame.rows)")
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(.secondary)

            Spacer()

            if let errorMessage = terminal.errorMessage {
                Text(errorMessage)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .lineLimit(1)
            }

            Button("Interrupt", action: terminal.sendControlC)
        }
    }
}

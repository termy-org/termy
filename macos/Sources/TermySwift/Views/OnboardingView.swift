import AppKit
import SwiftUI

/// First-run window: welcomes the user and offers to import settings from other
/// terminals. Shown once, gated by a UserDefaults flag.
@MainActor
final class OnboardingPresenter {
    static let shared = OnboardingPresenter()

    private var window: NSWindow?
    private let defaultsKey = "onboardingComplete"

    func presentIfNeeded() {
        guard !UserDefaults.standard.bool(forKey: defaultsKey) else {
            return
        }
        present()
    }

    func present() {
        if let window {
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        let view = OnboardingView(onFinish: { [weak self] in
            self?.finish()
        })
        let hosting = NSHostingController(rootView: view)
        let window = NSWindow(contentViewController: hosting)
        window.title = "Welcome to Termy"
        window.styleMask = [.titled, .closable]
        window.setContentSize(NSSize(width: 560, height: 480))
        window.center()
        window.isReleasedWhenClosed = false
        self.window = window
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func finish() {
        UserDefaults.standard.set(true, forKey: defaultsKey)
        window?.close()
        window = nil
    }
}

struct OnboardingView: View {
    let onFinish: () -> Void

    @State private var detected: [ImportableTerminal] = []
    @State private var importedFrom: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Welcome to Termy")
                    .font(.largeTitle.bold())
                Text("A fast, minimal terminal. Let's get you set up.")
                    .foregroundStyle(.secondary)
            }

            Divider()

            VStack(alignment: .leading, spacing: 10) {
                Text("Import settings")
                    .font(.headline)

                if detected.isEmpty {
                    Text("No other terminal configurations were found. You can change everything later in Settings.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                } else {
                    Text("We found these terminals. Import their font settings into Termy:")
                        .font(.callout)
                        .foregroundStyle(.secondary)

                    ForEach(detected) { terminal in
                        HStack {
                            Image(systemName: "terminal")
                                .foregroundStyle(.secondary)
                            Text(terminal.name)
                            Spacer()
                            Button("Import") {
                                importSettings(from: terminal)
                            }
                            .buttonStyle(.bordered)
                        }
                    }
                }

                if let importedFrom {
                    Label("Imported font settings from \(importedFrom).", systemImage: "checkmark.circle.fill")
                        .font(.callout)
                        .foregroundStyle(.green)
                }
            }

            Spacer()

            HStack {
                Spacer()
                Button("Get Started") {
                    onFinish()
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(28)
        .frame(minWidth: 520, minHeight: 440)
        .onAppear {
            detected = TerminalConfigImport.detected()
        }
    }

    private func importSettings(from terminal: ImportableTerminal) {
        let settings = TerminalConfigImport.read(terminal)
        guard !settings.isEmpty else {
            importedFrom = "\(terminal.name) (nothing to import)"
            return
        }
        TerminalConfigImport.apply(settings)
        importedFrom = terminal.name
    }
}

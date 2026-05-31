import Foundation

/// Optional "launch inside tmux" integration. The embedded Rust core spawns a
/// plain shell, so full tmux control-mode (as in the GPUI app) isn't available
/// here; instead, when enabled, each terminal execs into its own tmux session,
/// giving persistence, tmux splits, copy-mode, and the status line.
enum TmuxIntegration {
    /// Common install locations, since a GUI app's PATH may not include Homebrew.
    private static let defaultCandidatePaths = [
        "/opt/homebrew/bin/tmux",
        "/usr/local/bin/tmux",
        "/usr/bin/tmux",
    ]

    static var isEnabled: Bool {
        TermyAppConfiguration.loadFreshOrDefault().tmux.enabled
    }

    static var isAvailable: Bool {
        tmuxBinaryPath(for: TermyAppConfiguration.loadFreshOrDefault().tmux) != nil
    }

    /// Startup command that execs into a tmux session, or nil when tmux is
    /// disabled or not installed. `sessionHint` keeps each terminal in its own
    /// session so windows stay independent.
    static func startupCommand(sessionHint: String) -> String? {
        let tmux = TermyAppConfiguration.loadFreshOrDefault().tmux
        guard tmux.enabled, let binary = tmuxBinaryPath(for: tmux) else {
            return nil
        }
        let sessionSeed = tmux.persistence
            ? sessionHint
            : "\(sessionHint)-\(UUID().uuidString)"
        let session = sanitizedSessionName(sessionSeed)
        return "exec \(shellQuote(binary)) \(tmuxArguments(session: session, configuration: tmux).joined(separator: " "))"
    }

    static func tmuxBinaryPath(for tmux: TermyTmuxConfiguration) -> String? {
        let configured = tmux.binary.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !configured.isEmpty else {
            return firstExecutablePath(in: defaultCandidatePaths)
        }

        if configured.contains("/") {
            return FileManager.default.isExecutableFile(atPath: configured) ? configured : nil
        }

        let candidates = defaultCandidatePaths.map { path in
            URL(fileURLWithPath: path).deletingLastPathComponent().appendingPathComponent(configured).path
        }
        return firstExecutablePath(in: candidates) ?? configured
    }

    private static func firstExecutablePath(in paths: [String]) -> String? {
        paths.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    private static func sanitizedSessionName(_ hint: String) -> String {
        let allowed = CharacterSet(charactersIn:
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_")
        let cleaned = String(hint.unicodeScalars.filter { allowed.contains($0) })
        return cleaned.isEmpty ? "termy" : "termy-\(cleaned)"
    }

    private static func tmuxArguments(
        session: String,
        configuration: TermyTmuxConfiguration
    ) -> [String] {
        var args = ["new-session"]
        if configuration.persistence {
            args.append("-A")
        }
        args.append(contentsOf: ["-s", session])

        let target = "\(session):*"
        for (option, value) in managedSessionWindowOptionOverrides(configuration) {
            args.append("\\;")
            args.append(contentsOf: ["set-window-option", "-q", "-t", target, option, value])
        }
        return args.map(shellQuoteTmuxArgument)
    }

    private static func managedSessionWindowOptionOverrides(
        _ tmux: TermyTmuxConfiguration
    ) -> [(String, String)] {
        var overrides = [
            ("pane-border-status", "off"),
            ("pane-border-format", ""),
        ]
        if !tmux.showActivePaneBorder {
            overrides.append(contentsOf: [
                ("pane-border-indicators", "off"),
                ("pane-border-style", "fg=default,bg=default"),
                ("pane-active-border-style", "fg=default,bg=default"),
            ])
        }
        return overrides
    }

    private static func shellQuoteTmuxArgument(_ value: String) -> String {
        value == "\\;" ? value : shellQuote(value)
    }

    private static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }
}

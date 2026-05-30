import Foundation

/// Optional "launch inside tmux" integration. The embedded Rust core spawns a
/// plain shell, so full tmux control-mode (as in the GPUI app) isn't available
/// here; instead, when enabled, each terminal execs into its own tmux session,
/// giving persistence, tmux splits, copy-mode, and the status line.
enum TmuxIntegration {
    private static let defaultsKey = "tmuxEnabled"

    static var isEnabled: Bool {
        get { UserDefaults.standard.bool(forKey: defaultsKey) }
        set { UserDefaults.standard.set(newValue, forKey: defaultsKey) }
    }

    /// Common install locations, since a GUI app's PATH may not include Homebrew.
    private static let candidatePaths = [
        "/opt/homebrew/bin/tmux",
        "/usr/local/bin/tmux",
        "/usr/bin/tmux",
    ]

    static func tmuxBinaryPath() -> String? {
        candidatePaths.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    static var isAvailable: Bool {
        tmuxBinaryPath() != nil
    }

    /// Startup command that execs into a tmux session, or nil when tmux is
    /// disabled or not installed. `sessionHint` keeps each terminal in its own
    /// session so windows stay independent.
    static func startupCommand(sessionHint: String) -> String? {
        guard isEnabled, let binary = tmuxBinaryPath() else {
            return nil
        }
        let session = sanitizedSessionName(sessionHint)
        return "exec \(shellQuote(binary)) new-session -A -s \(shellQuote(session))"
    }

    private static func sanitizedSessionName(_ hint: String) -> String {
        let allowed = CharacterSet(charactersIn:
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_")
        let cleaned = String(hint.unicodeScalars.filter { allowed.contains($0) })
        return cleaned.isEmpty ? "termy" : "termy-\(cleaned)"
    }

    private static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }
}

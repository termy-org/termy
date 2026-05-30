import Foundation

/// Settings imported from another terminal's config. Currently the portable
/// subset: font family and size.
struct ImportedSettings: Equatable {
    var fontFamily: String?
    var fontSize: Double?

    var rootValues: [String: String] {
        var values: [String: String] = [:]
        if let fontFamily, !fontFamily.isEmpty {
            values["font_family"] = fontFamily
        }
        if let fontSize, fontSize > 0 {
            values["font_size"] = String(format: "%g", fontSize)
        }
        return values
    }

    var isEmpty: Bool {
        rootValues.isEmpty
    }
}

struct ImportableTerminal: Identifiable {
    let id: String
    let name: String
    let configPath: URL

    var isDetected: Bool {
        FileManager.default.fileExists(atPath: configPath.path)
    }
}

/// Detects and imports settings from other terminal emulators' config files.
enum TerminalConfigImport {
    static func candidates() -> [ImportableTerminal] {
        let home = FileManager.default.homeDirectoryForCurrentUser
        func path(_ relative: String) -> URL {
            home.appendingPathComponent(relative)
        }
        return [
            ImportableTerminal(
                id: "alacritty",
                name: "Alacritty",
                configPath: path(".config/alacritty/alacritty.toml")
            ),
            ImportableTerminal(
                id: "kitty",
                name: "Kitty",
                configPath: path(".config/kitty/kitty.conf")
            ),
            ImportableTerminal(
                id: "ghostty",
                name: "Ghostty",
                configPath: path(".config/ghostty/config")
            ),
        ]
    }

    static func detected() -> [ImportableTerminal] {
        candidates().filter(\.isDetected)
    }

    static func read(_ terminal: ImportableTerminal) -> ImportedSettings {
        guard let text = try? String(contentsOf: terminal.configPath, encoding: .utf8) else {
            return ImportedSettings()
        }
        switch terminal.id {
        case "alacritty":
            return parseAlacritty(text)
        case "kitty":
            return parseKitty(text)
        case "ghostty":
            return parseGhostty(text)
        default:
            return ImportedSettings()
        }
    }

    /// Apply imported settings to the Termy config, returning the keys written.
    @discardableResult
    static func apply(_ settings: ImportedSettings) -> [String] {
        var written: [String] = []
        for (key, value) in settings.rootValues {
            if (try? SettingsBridge.setRoot(key: key, value: value)) != nil {
                written.append(key)
            }
        }
        if !written.isEmpty {
            NotificationCenter.default.post(name: .termySettingsChanged, object: nil)
        }
        return written
    }

    // MARK: - Parsers

    private static func parseAlacritty(_ text: String) -> ImportedSettings {
        // TOML: `family = "JetBrains Mono"` (under [font.normal]) and `size = 14`.
        var result = ImportedSettings()
        if let family = firstMatch(in: text, pattern: #"(?m)^\s*family\s*=\s*"([^"]+)""#) {
            result.fontFamily = family
        }
        if let size = firstMatch(in: text, pattern: #"(?m)^\s*size\s*=\s*([0-9]+(?:\.[0-9]+)?)"#) {
            result.fontSize = Double(size)
        }
        return result
    }

    private static func parseKitty(_ text: String) -> ImportedSettings {
        // Space-separated: `font_family JetBrains Mono`, `font_size 14.0`.
        var result = ImportedSettings()
        if let family = firstMatch(in: text, pattern: #"(?m)^\s*font_family\s+(.+?)\s*$"#) {
            result.fontFamily = family
        }
        if let size = firstMatch(in: text, pattern: #"(?m)^\s*font_size\s+([0-9]+(?:\.[0-9]+)?)"#) {
            result.fontSize = Double(size)
        }
        return result
    }

    private static func parseGhostty(_ text: String) -> ImportedSettings {
        // `key = value`: `font-family = JetBrains Mono`, `font-size = 14`.
        var result = ImportedSettings()
        if let family = firstMatch(in: text, pattern: #"(?m)^\s*font-family\s*=\s*(.+?)\s*$"#) {
            result.fontFamily = family.trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
        }
        if let size = firstMatch(in: text, pattern: #"(?m)^\s*font-size\s*=\s*([0-9]+(?:\.[0-9]+)?)"#) {
            result.fontSize = Double(size)
        }
        return result
    }

    private static func firstMatch(in text: String, pattern: String) -> String? {
        guard let regex = try? NSRegularExpression(pattern: pattern) else {
            return nil
        }
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        guard let match = regex.firstMatch(in: text, range: range),
              match.numberOfRanges > 1,
              let captureRange = Range(match.range(at: 1), in: text)
        else {
            return nil
        }
        return String(text[captureRange]).trimmingCharacters(in: .whitespaces)
    }
}

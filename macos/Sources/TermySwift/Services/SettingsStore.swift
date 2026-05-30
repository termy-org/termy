import SwiftUI

@MainActor
final class SettingsStore: ObservableObject {
    @Published private(set) var schema: SettingsSchema?
    @Published var errorMessage: String?

    /// Canonical current values, keyed by setting key. Edited optimistically and
    /// written straight through to the config file via `SettingsBridge`.
    @Published private(set) var values: [String: String] = [:]
    /// Color overrides keyed by color key. Empty string means "inherit theme".
    @Published private(set) var colors: [String: String] = [:]
    @Published var keybindsText: String = ""

    func load() {
        do {
            let schema = try SettingsBridge.loadSchema()
            var values: [String: String] = [:]
            var colors: [String: String] = [:]
            for section in schema.sections {
                for group in section.groups ?? [] {
                    for setting in group.settings {
                        values[setting.key] = setting.value ?? ""
                    }
                }
                for color in section.colors ?? [] {
                    colors[color.key] = color.hex ?? ""
                }
                if let keybinds = section.keybinds {
                    keybindsText = keybinds.joined(separator: "\n")
                }
            }
            self.values = values
            self.colors = colors
            self.schema = schema
            errorMessage = nil
        } catch {
            report(error)
        }
    }

    func section(id: String?) -> SettingsSectionModel? {
        guard let id else {
            return nil
        }
        return schema?.sections.first { $0.id == id }
    }

    func value(for key: String) -> String {
        values[key] ?? ""
    }

    func commitRoot(key: String, value: String) {
        values[key] = value
        commit {
            if Self.shouldResetRootSetting(key: key, value: value) {
                try SettingsBridge.resetRoot(key: key)
            } else {
                try SettingsBridge.setRoot(key: key, value: value)
            }
        }
    }

    func boolBinding(_ key: String) -> Binding<Bool> {
        Binding(
            get: { self.values[key] == "true" },
            set: { self.commitRoot(key: key, value: $0 ? "true" : "false") }
        )
    }

    func enumBinding(_ key: String) -> Binding<String> {
        Binding(
            get: { self.values[key] ?? "" },
            set: { self.commitRoot(key: key, value: $0) }
        )
    }

    func colorHex(for key: String) -> String {
        colors[key] ?? ""
    }

    func commitColor(key: String, hex: String?) {
        colors[key] = hex ?? ""
        commit {
            try SettingsBridge.setColor(key: key, hex: hex)
        }
    }

    func commitKeybinds() {
        commit {
            try SettingsBridge.setKeybinds(keybindsText)
        }
    }

    private func commit(_ write: () throws -> Void) {
        do {
            try write()
            notifyChanged()
        } catch {
            report(error)
        }
    }

    private func notifyChanged() {
        NotificationCenter.default.post(name: .termySettingsChanged, object: nil)
    }

    private func report(_ error: Error) {
        errorMessage = String(describing: error)
    }

    private static func shouldResetRootSetting(key: String, value: String) -> Bool {
        guard value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return false
        }
        return [
            "working_dir",
            "shell",
            "colorterm",
            "inactive_tab_scrollback",
        ].contains(key)
    }
}

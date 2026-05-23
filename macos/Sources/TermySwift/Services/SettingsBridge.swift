import CTermy
import Foundation

/// Thin wrapper over the libtermy settings C functions. All writes target the
/// shared config file at `~/.config/termy/config.txt` and preserve comments and
/// formatting (the Rust side does surgical edits).
enum SettingsBridge {
    enum BridgeError: Error, CustomStringConvertible {
        case decode(String)

        var description: String {
            switch self {
            case let .decode(message):
                return message
            }
        }
    }

    static func loadSchema() throws -> SettingsSchema {
        var config: OpaquePointer?
        try TermyFfiBridge.requireOK("termy_config_load_default", termy_config_load_default(&config))
        defer {
            if let config {
                _ = termy_config_free(config)
            }
        }

        var bytes = TermyFfiBytes()
        try TermyFfiBridge.requireOK("termy_settings_schema_json", termy_settings_schema_json(config, &bytes))
        defer {
            if bytes.ptr != nil {
                _ = termy_buffer_free(bytes)
            }
        }

        guard let ptr = bytes.ptr, bytes.len > 0 else {
            throw BridgeError.decode("settings schema was empty")
        }
        let data = Data(bytes: ptr, count: Int(bytes.len))
        return try JSONDecoder().decode(SettingsSchema.self, from: data)
    }

    static func setRoot(key: String, value: String) throws {
        let keyBytes = Array(key.utf8)
        let valueBytes = Array(value.utf8)
        let status = keyBytes.withUnsafeBufferPointer { keyBuffer in
            valueBytes.withUnsafeBufferPointer { valueBuffer in
                termy_settings_set_root(
                    keyBuffer.baseAddress,
                    keyBuffer.count,
                    valueBuffer.baseAddress,
                    valueBuffer.count
                )
            }
        }
        try TermyFfiBridge.requireOK("termy_settings_set_root", status)
    }

    /// Pass `hex == nil` to clear the override and inherit the theme color.
    static func setColor(key: String, hex: String?) throws {
        let keyBytes = Array(key.utf8)
        let hexBytes = hex.map { Array($0.utf8) } ?? []
        let status = keyBytes.withUnsafeBufferPointer { keyBuffer in
            hexBytes.withUnsafeBufferPointer { hexBuffer in
                termy_settings_set_color(
                    keyBuffer.baseAddress,
                    keyBuffer.count,
                    hex == nil ? nil : hexBuffer.baseAddress,
                    hex == nil ? 0 : hexBuffer.count
                )
            }
        }
        try TermyFfiBridge.requireOK("termy_settings_set_color", status)
    }

    static func setKeybinds(_ text: String) throws {
        let textBytes = Array(text.utf8)
        let status = textBytes.withUnsafeBufferPointer { textBuffer in
            termy_settings_set_keybinds(textBuffer.baseAddress, textBuffer.count)
        }
        try TermyFfiBridge.requireOK("termy_settings_set_keybinds", status)
    }
}

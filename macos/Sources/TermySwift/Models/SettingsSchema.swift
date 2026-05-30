import Foundation

/// Decoded representation of the JSON returned by `termy_settings_schema_json`.
struct SettingsSchema: Decodable {
    var sections: [SettingsSectionModel]
}

struct SettingsSectionModel: Decodable, Identifiable {
    var id: String
    var label: String
    var systemImage: String
    var groups: [SettingsGroup]?
    var colors: [ColorSetting]?
    var keybinds: [String]?
}

struct SettingsGroup: Decodable, Identifiable {
    var label: String
    var settings: [Setting]

    var id: String { label }
}

enum SettingKind: String, Decodable {
    case text
    case numeric
    case boolean
    case enumeration = "enum"
    case special
}

struct Setting: Decodable, Identifiable {
    var key: String
    var title: String
    var description: String
    var kind: SettingKind
    var value: String?
    var choices: [SettingEnumChoice]?

    var id: String { key }
}

struct SettingEnumChoice: Decodable, Identifiable, Hashable {
    var value: String
    var label: String

    var id: String { value }
}

struct ColorSetting: Decodable, Identifiable {
    var key: String
    var title: String
    var description: String
    var hex: String?

    var id: String { key }
}

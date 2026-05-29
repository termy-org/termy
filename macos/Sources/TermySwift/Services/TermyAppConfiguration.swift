import CTermy
import CoreGraphics
import Foundation

enum TermyAppConfigurationError: Error, CustomStringConvertible {
    case missingConfig

    var description: String {
        switch self {
        case .missingConfig:
            return "libtermy did not return a config handle"
        }
    }
}

struct TermyAppConfiguration {
    var windowWidth: CGFloat
    var windowHeight: CGFloat
    var safety: TermySafetyConfiguration
    var native: TermyNativeConfiguration
    var configPath: String?
    var tasks: [TermyTaskConfiguration]
    var keybinds: [TermyKeybindConfiguration]

    var windowSize: CGSize {
        CGSize(width: windowWidth, height: windowHeight)
    }

    private static let defaultConfiguration = TermyAppConfiguration(
        windowWidth: 1280,
        windowHeight: 820,
        safety: .default,
        native: .default,
        configPath: nil,
        tasks: [],
        keybinds: []
    )

    private static let loadedConfiguration = Result {
        try load()
    }

    static let current: TermyAppConfiguration = {
        switch loadedConfiguration {
        case .success(let configuration):
            return configuration
        case .failure:
            return defaultConfiguration
        }
    }()

    static let loadErrorMessage: String? = {
        switch loadedConfiguration {
        case .success:
            return nil
        case .failure(let error):
            return String(describing: error)
        }
    }()

    private static func load() throws -> TermyAppConfiguration {
        var config: OpaquePointer?
        try TermyFfiBridge.requireOK("termy_config_load_default", termy_config_load_default(&config))
        guard let config else {
            throw TermyAppConfigurationError.missingConfig
        }
        defer {
            _ = termy_config_free(config)
        }

        var width: Float = Float(defaultConfiguration.windowWidth)
        var height: Float = Float(defaultConfiguration.windowHeight)
        try TermyFfiBridge.requireOK(
            "termy_config_window_size",
            termy_config_window_size(config, &width, &height)
        )

        var safety = TermyFfiSafetyConfig()
        try TermyFfiBridge.requireOK(
            "termy_config_safety",
            termy_config_safety(config, &safety)
        )

        var native = TermyFfiNativeConfig()
        try TermyFfiBridge.requireOK(
            "termy_config_native",
            termy_config_native(config, &native)
        )

        var configPath = TermyFfiBytes()
        try TermyFfiBridge.requireOK(
            "termy_config_path",
            termy_config_path(config, &configPath)
        )
        defer {
            if configPath.ptr != nil {
                _ = termy_buffer_free(configPath)
            }
        }

        var tasksJSON = TermyFfiBytes()
        try TermyFfiBridge.requireOK(
            "termy_config_tasks_json",
            termy_config_tasks_json(config, &tasksJSON)
        )
        defer {
            if tasksJSON.ptr != nil {
                _ = termy_buffer_free(tasksJSON)
            }
        }
        let tasksData = Data(TermyFfiBridge.string(from: tasksJSON)?.utf8 ?? "".utf8)
        let tasks = try JSONDecoder().decode([TermyTaskConfiguration].self, from: tasksData)

        var keybindsJSON = TermyFfiBytes()
        try TermyFfiBridge.requireOK(
            "termy_config_keybinds_json",
            termy_config_keybinds_json(config, &keybindsJSON)
        )
        defer {
            if keybindsJSON.ptr != nil {
                _ = termy_buffer_free(keybindsJSON)
            }
        }
        let keybindsData = Data(TermyFfiBridge.string(from: keybindsJSON)?.utf8 ?? "".utf8)
        let keybinds = try JSONDecoder().decode([TermyKeybindConfiguration].self, from: keybindsData)

        return TermyAppConfiguration(
            windowWidth: CGFloat(max(320, width)),
            windowHeight: CGFloat(max(240, height)),
            safety: TermySafetyConfiguration(safety),
            native: TermyNativeConfiguration(native),
            configPath: TermyFfiBridge.string(from: configPath),
            tasks: tasks,
            keybinds: keybinds
        )
    }

}

struct TermySafetyConfiguration {
    var warnOnQuit: Bool
    var warnOnQuitWithRunningProcess: Bool

    static let `default` = TermySafetyConfiguration(
        warnOnQuit: false,
        warnOnQuitWithRunningProcess: true
    )

    init(warnOnQuit: Bool, warnOnQuitWithRunningProcess: Bool) {
        self.warnOnQuit = warnOnQuit
        self.warnOnQuitWithRunningProcess = warnOnQuitWithRunningProcess
    }

    init(_ ffiConfig: TermyFfiSafetyConfig) {
        warnOnQuit = ffiConfig.warn_on_quit
        warnOnQuitWithRunningProcess = ffiConfig.warn_on_quit_with_running_process
    }

    static func loadCurrent() -> TermySafetyConfiguration {
        do {
            return try TermyAppConfiguration.loadFresh().safety
        } catch {
            return .default
        }
    }
}

struct TermyNativeConfiguration {
    var simpleMode: Bool
    var nativeTabPersistence: Bool
    var nativeLayoutAutosave: Bool
    var nativeBufferPersistence: Bool
    var chromeContrast: Bool
    var commandPaletteShowKeybinds: Bool
    var appIcon: TermyAppIcon
    var shellIntegrationEnabled: Bool
    var progressIndicatorEnabled: Bool
    var autoHideTabbar: Bool
    var showTermyInTitlebar: Bool

    static let `default` = TermyNativeConfiguration(
        simpleMode: false,
        nativeTabPersistence: false,
        nativeLayoutAutosave: false,
        nativeBufferPersistence: false,
        chromeContrast: false,
        commandPaletteShowKeybinds: true,
        appIcon: .default,
        shellIntegrationEnabled: true,
        progressIndicatorEnabled: true,
        autoHideTabbar: true,
        showTermyInTitlebar: true
    )

    init(
        simpleMode: Bool,
        nativeTabPersistence: Bool,
        nativeLayoutAutosave: Bool,
        nativeBufferPersistence: Bool,
        chromeContrast: Bool,
        commandPaletteShowKeybinds: Bool,
        appIcon: TermyAppIcon,
        shellIntegrationEnabled: Bool,
        progressIndicatorEnabled: Bool,
        autoHideTabbar: Bool,
        showTermyInTitlebar: Bool
    ) {
        self.simpleMode = simpleMode
        self.nativeTabPersistence = nativeTabPersistence
        self.nativeLayoutAutosave = nativeLayoutAutosave
        self.nativeBufferPersistence = nativeBufferPersistence
        self.chromeContrast = chromeContrast
        self.commandPaletteShowKeybinds = commandPaletteShowKeybinds
        self.appIcon = appIcon
        self.shellIntegrationEnabled = shellIntegrationEnabled
        self.progressIndicatorEnabled = progressIndicatorEnabled
        self.autoHideTabbar = autoHideTabbar
        self.showTermyInTitlebar = showTermyInTitlebar
    }

    init(_ ffiConfig: TermyFfiNativeConfig) {
        simpleMode = ffiConfig.simple_mode
        nativeTabPersistence = ffiConfig.native_tab_persistence
        nativeLayoutAutosave = ffiConfig.native_layout_autosave
        nativeBufferPersistence = ffiConfig.native_buffer_persistence
        chromeContrast = ffiConfig.chrome_contrast
        commandPaletteShowKeybinds = ffiConfig.command_palette_show_keybinds
        appIcon = TermyAppIcon(rawValue: ffiConfig.app_icon) ?? .default
        shellIntegrationEnabled = ffiConfig.shell_integration_enabled
        progressIndicatorEnabled = ffiConfig.progress_indicator_enabled
        autoHideTabbar = ffiConfig.auto_hide_tabbar
        showTermyInTitlebar = ffiConfig.show_termy_in_titlebar
    }
}

enum TermyAppIcon: UInt32 {
    case `default` = 0
    case old = 1
}

struct TermyTaskConfiguration: Codable, Equatable, Identifiable, Hashable {
    var name: String
    var command: String
    var layout: String?
    var workingDirectory: String?

    var id: String {
        name
    }

    enum CodingKeys: String, CodingKey {
        case name
        case command
        case layout
        case workingDirectory = "working_dir"
    }
}

struct TermyKeybindConfiguration: Codable, Equatable, Hashable {
    var trigger: String
    var action: String
}

private extension TermyAppConfiguration {
    static func loadFresh() throws -> TermyAppConfiguration {
        try load()
    }
}

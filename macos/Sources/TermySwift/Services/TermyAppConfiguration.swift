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
    var tmux: TermyTmuxConfiguration
    var native: TermyNativeConfiguration
    var uiFontFamily: String
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
        tmux: .default,
        native: .default,
        uiFontFamily: "JetBrains Mono",
        configPath: nil,
        tasks: [],
        keybinds: []
    )

    private static let loadedConfiguration = Result {
        try load()
    }

    static let current: TermyAppConfiguration = {
        cachedLoadedOrDefault()
    }()

    static func loadFreshOrDefault() -> TermyAppConfiguration {
        (try? load()) ?? defaultConfiguration
    }

    static func loadFresh() throws -> TermyAppConfiguration {
        try load()
    }

    static let loadErrorMessage: String? = {
        switch loadedConfiguration {
        case .success:
            return nil
        case .failure(let error):
            return String(describing: error)
        }
    }()

    private static func cachedLoadedOrDefault() -> TermyAppConfiguration {
        switch loadedConfiguration {
        case .success(let configuration):
            return configuration
        case .failure:
            return defaultConfiguration
        }
    }

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

        var tmuxBinary = TermyFfiBytes()
        try TermyFfiBridge.requireOK(
            "termy_config_tmux_binary",
            termy_config_tmux_binary(config, &tmuxBinary)
        )
        defer {
            if tmuxBinary.ptr != nil {
                _ = termy_buffer_free(tmuxBinary)
            }
        }

        var uiFontFamily = TermyFfiBytes()
        try TermyFfiBridge.requireOK(
            "termy_config_ui_font_family",
            termy_config_ui_font_family(config, &uiFontFamily)
        )
        defer {
            if uiFontFamily.ptr != nil {
                _ = termy_buffer_free(uiFontFamily)
            }
        }

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
            tmux: TermyTmuxConfiguration(native, binary: TermyFfiBridge.string(from: tmuxBinary) ?? "tmux"),
            native: TermyNativeConfiguration(native),
            uiFontFamily: Self.normalizedUIFontFamily(
                TermyFfiBridge.string(from: uiFontFamily) ?? defaultConfiguration.uiFontFamily
            ),
            configPath: TermyFfiBridge.string(from: configPath),
            tasks: tasks,
            keybinds: keybinds
        )
    }

    private static func normalizedUIFontFamily(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? defaultConfiguration.uiFontFamily : trimmed
    }

}

struct TermyTmuxConfiguration {
    var enabled: Bool
    var persistence: Bool
    var binary: String
    var showActivePaneBorder: Bool

    static let `default` = TermyTmuxConfiguration(
        enabled: false,
        persistence: true,
        binary: "tmux",
        showActivePaneBorder: true
    )

    init(
        enabled: Bool,
        persistence: Bool,
        binary: String,
        showActivePaneBorder: Bool
    ) {
        self.enabled = enabled
        self.persistence = persistence
        self.binary = binary
        self.showActivePaneBorder = showActivePaneBorder
    }

    init(_ ffiConfig: TermyFfiNativeConfig, binary: String) {
        enabled = ffiConfig.tmux_enabled
        persistence = ffiConfig.tmux_persistence
        self.binary = binary
        showActivePaneBorder = ffiConfig.tmux_show_active_pane_border
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
    var autoUpdate: Bool
    var simpleMode: Bool
    var nativeTabPersistence: Bool
    var nativeLayoutAutosave: Bool
    var nativeBufferPersistence: Bool
    var showDebugOverlay: Bool
    var onboardingComplete: Bool
    var tabCloseVisibility: TermyTabCloseVisibility
    var tabWidthMode: TermyTabWidthMode
    var tabBarPosition: TermyTabBarPosition
    var tabSwitchModifierHints: Bool
    var chromeContrast: Bool
    var commandPaletteShowKeybinds: Bool
    var appIcon: TermyAppIcon
    var shellIntegrationEnabled: Bool
    var progressIndicatorEnabled: Bool
    var autoHideTabbar: Bool
    var showTermyInTitlebar: Bool

    static let `default` = TermyNativeConfiguration(
        autoUpdate: true,
        simpleMode: false,
        nativeTabPersistence: false,
        nativeLayoutAutosave: false,
        nativeBufferPersistence: false,
        showDebugOverlay: false,
        onboardingComplete: true,
        tabCloseVisibility: .hover,
        tabWidthMode: .uniform,
        tabBarPosition: .top,
        tabSwitchModifierHints: true,
        chromeContrast: false,
        commandPaletteShowKeybinds: true,
        appIcon: .old,
        shellIntegrationEnabled: true,
        progressIndicatorEnabled: true,
        autoHideTabbar: true,
        showTermyInTitlebar: true
    )

    init(
        autoUpdate: Bool,
        simpleMode: Bool,
        nativeTabPersistence: Bool,
        nativeLayoutAutosave: Bool,
        nativeBufferPersistence: Bool,
        showDebugOverlay: Bool,
        onboardingComplete: Bool,
        tabCloseVisibility: TermyTabCloseVisibility,
        tabWidthMode: TermyTabWidthMode,
        tabBarPosition: TermyTabBarPosition,
        tabSwitchModifierHints: Bool,
        chromeContrast: Bool,
        commandPaletteShowKeybinds: Bool,
        appIcon: TermyAppIcon,
        shellIntegrationEnabled: Bool,
        progressIndicatorEnabled: Bool,
        autoHideTabbar: Bool,
        showTermyInTitlebar: Bool
    ) {
        self.autoUpdate = autoUpdate
        self.simpleMode = simpleMode
        self.nativeTabPersistence = nativeTabPersistence
        self.nativeLayoutAutosave = nativeLayoutAutosave
        self.nativeBufferPersistence = nativeBufferPersistence
        self.showDebugOverlay = showDebugOverlay
        self.onboardingComplete = onboardingComplete
        self.tabCloseVisibility = tabCloseVisibility
        self.tabWidthMode = tabWidthMode
        self.tabBarPosition = tabBarPosition
        self.tabSwitchModifierHints = tabSwitchModifierHints
        self.chromeContrast = chromeContrast
        self.commandPaletteShowKeybinds = commandPaletteShowKeybinds
        self.appIcon = appIcon
        self.shellIntegrationEnabled = shellIntegrationEnabled
        self.progressIndicatorEnabled = progressIndicatorEnabled
        self.autoHideTabbar = autoHideTabbar
        self.showTermyInTitlebar = showTermyInTitlebar
    }

    init(_ ffiConfig: TermyFfiNativeConfig) {
        autoUpdate = ffiConfig.auto_update
        simpleMode = ffiConfig.simple_mode
        nativeTabPersistence = ffiConfig.native_tab_persistence
        nativeLayoutAutosave = ffiConfig.native_layout_autosave
        nativeBufferPersistence = ffiConfig.native_buffer_persistence
        showDebugOverlay = ffiConfig.show_debug_overlay
        onboardingComplete = ffiConfig.onboarding_complete
        tabCloseVisibility = TermyTabCloseVisibility(rawValue: ffiConfig.tab_close_visibility) ?? .hover
        tabWidthMode = TermyTabWidthMode(rawValue: ffiConfig.tab_width_mode) ?? .uniform
        tabBarPosition = TermyTabBarPosition(rawValue: ffiConfig.tab_bar_position) ?? .top
        tabSwitchModifierHints = ffiConfig.tab_switch_modifier_hints
        chromeContrast = ffiConfig.chrome_contrast
        commandPaletteShowKeybinds = ffiConfig.command_palette_show_keybinds
        appIcon = TermyAppIcon(rawValue: ffiConfig.app_icon) ?? .old
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

enum TermyTabCloseVisibility: UInt32 {
    case activeHover = 0
    case hover = 1
    case always = 2
}

enum TermyTabWidthMode: UInt32 {
    case stable = 0
    case activeGrow = 1
    case activeGrowSticky = 2
    case uniform = 3
}

enum TermyTabBarPosition: UInt32 {
    case top = 0
    case right = 1
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

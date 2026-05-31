import XCTest
@testable import TermySwift

final class TermyConfigurationParityTests: XCTestCase {
    func testSwiftConfigurationLoadsSharedNativeSafetyTasksAndKeybinds() throws {
        let configuration = try TermyAppConfiguration.load(contents: """
        window_width = 1440
        window_height = 900
        warn_on_quit = true
        warn_on_quit_with_running_process = false
        auto_update = false
        tmux_enabled = true
        tmux_persistence = false
        tmux_binary = /opt/homebrew/bin/tmux
        tmux_show_active_pane_border = false
        simple_mode = true
        native_tab_persistence = true
        native_layout_autosave = true
        native_buffer_persistence = true
        show_debug_overlay = true
        onboarding_complete = false
        tab_close_visibility = always
        tab_width_mode = active_grow_sticky
        tab_bar_position = right
        tab_switch_modifier_hints = false
        ui_font_family = Avenir Next
        chrome_contrast = true
        command_palette_show_keybinds = false
        app_icon = old
        shell_integration_enabled = false
        progress_indicator_enabled = false
        auto_hide_tabbar = false
        show_termy_in_titlebar = false
        task.build.command = cargo build
        task.build.working_dir = crates/cli
        task.dev_server.layout = dashboard
        task.dev_server.command = cargo run
        keybind = clear
        keybind = cmd-p=toggle_command_palette
        keybind = cmd-d=split_pane_vertical
        """)

        XCTAssertEqual(configuration.windowWidth, 1440)
        XCTAssertEqual(configuration.windowHeight, 900)
        XCTAssertEqual(configuration.safety.warnOnQuit, true)
        XCTAssertEqual(configuration.safety.warnOnQuitWithRunningProcess, false)
        XCTAssertEqual(configuration.tmux.enabled, true)
        XCTAssertEqual(configuration.tmux.persistence, false)
        XCTAssertEqual(configuration.tmux.binary, "/opt/homebrew/bin/tmux")
        XCTAssertEqual(configuration.tmux.showActivePaneBorder, false)

        let native = configuration.native
        XCTAssertEqual(native.autoUpdate, false)
        XCTAssertEqual(native.simpleMode, true)
        XCTAssertEqual(native.nativeTabPersistence, true)
        XCTAssertEqual(native.nativeLayoutAutosave, true)
        XCTAssertEqual(native.nativeBufferPersistence, true)
        XCTAssertEqual(native.showDebugOverlay, true)
        XCTAssertEqual(native.onboardingComplete, false)
        XCTAssertEqual(native.tabCloseVisibility, .always)
        XCTAssertEqual(native.tabWidthMode, .activeGrowSticky)
        XCTAssertEqual(native.tabBarPosition, .right)
        XCTAssertEqual(native.tabSwitchModifierHints, false)
        XCTAssertEqual(native.chromeContrast, true)
        XCTAssertEqual(native.commandPaletteShowKeybinds, false)
        XCTAssertEqual(native.appIcon, .old)
        XCTAssertEqual(native.shellIntegrationEnabled, false)
        XCTAssertEqual(native.progressIndicatorEnabled, false)
        XCTAssertEqual(native.autoHideTabbar, false)
        XCTAssertEqual(native.showTermyInTitlebar, false)
        XCTAssertEqual(configuration.uiFontFamily, "Avenir Next")

        XCTAssertEqual(configuration.tasks, [
            TermyTaskConfiguration(
                name: "build",
                command: "cargo build",
                layout: nil,
                workingDirectory: "crates/cli"
            ),
            TermyTaskConfiguration(
                name: "dev_server",
                command: "cargo run",
                layout: "dashboard",
                workingDirectory: nil
            )
        ])
        XCTAssertEqual(configuration.keybinds, [
            TermyKeybindConfiguration(trigger: "cmd-p", action: "toggle_command_palette"),
            TermyKeybindConfiguration(trigger: "cmd-d", action: "split_pane_vertical")
        ])
    }

    func testConfigurationMatrixCoversNativeTabAndIconVariants() throws {
        let cases: [(name: String, contents: String, assert: (TermyAppConfiguration) -> Void)] = [
            (
                name: "left defaults",
                contents: "",
                assert: { configuration in
                    XCTAssertEqual(configuration.native.tabCloseVisibility, .hover)
                    XCTAssertEqual(configuration.native.tabWidthMode, .uniform)
                    XCTAssertEqual(configuration.native.tabBarPosition, .top)
                    XCTAssertEqual(configuration.native.appIcon, .old)
                }
            ),
            (
                name: "stable top default icon",
                contents: """
                tab_close_visibility = active_hover
                tab_width_mode = stable
                tab_bar_position = top
                app_icon = default
                """,
                assert: { configuration in
                    XCTAssertEqual(configuration.native.tabCloseVisibility, .activeHover)
                    XCTAssertEqual(configuration.native.tabWidthMode, .stable)
                    XCTAssertEqual(configuration.native.tabBarPosition, .top)
                    XCTAssertEqual(configuration.native.appIcon, .default)
                }
            ),
            (
                name: "active grow right sidebar",
                contents: """
                tab_close_visibility = hover
                tab_width_mode = active_grow
                tab_bar_position = right
                app_icon = old
                """,
                assert: { configuration in
                    XCTAssertEqual(configuration.native.tabCloseVisibility, .hover)
                    XCTAssertEqual(configuration.native.tabWidthMode, .activeGrow)
                    XCTAssertEqual(configuration.native.tabBarPosition, .right)
                    XCTAssertEqual(configuration.native.appIcon, .old)
                }
            )
        ]

        for testCase in cases {
            let configuration = try TermyAppConfiguration.load(contents: testCase.contents)
            testCase.assert(configuration)
        }
    }

    func testEmptyUIFontFallsBackToDefault() throws {
        let configuration = try TermyAppConfiguration.load(contents: "ui_font_family =     \n")
        XCTAssertEqual(configuration.uiFontFamily, "JetBrains Mono")
    }
}

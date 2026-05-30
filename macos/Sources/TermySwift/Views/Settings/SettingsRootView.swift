import AppKit
import SwiftUI

struct SettingsRootView: View {
    static let appearanceSectionID = "appearance"

    @StateObject private var store = SettingsStore()
    @State private var selection: String?

    /// Native-only section (not part of the FFI settings schema) that hosts the
    /// app logo switcher.
    private let appearanceSection = SettingsSectionModel(
        id: SettingsRootView.appearanceSectionID,
        label: "Appearance",
        systemImage: "paintbrush",
        groups: nil,
        colors: nil,
        keybinds: nil
    )

    var body: some View {
        NavigationSplitView {
            SettingsSidebarView(sections: supportedSections, selection: $selection)
        } detail: {
            SettingsDetailView(section: selectedSection, store: store)
        }
        .frame(minWidth: 760, minHeight: 520)
        .onAppear {
            store.load()
            selectDefaultSectionIfNeeded()
        }
        .onChange(of: store.schema?.sections.map(\.id) ?? []) { _, _ in
            selectDefaultSectionIfNeeded()
        }
        .alert(
            "Settings Error",
            isPresented: Binding(
                get: { store.errorMessage != nil },
                set: { if !$0 { store.errorMessage = nil } }
            )
        ) {
            Button("OK", role: .cancel) { store.errorMessage = nil }
        } message: {
            Text(store.errorMessage ?? "")
        }
    }

    private var supportedSections: [SettingsSectionModel] {
        let schemaSections = store.schema?.sections.filter(\.hasSupportedSettings) ?? []
        return schemaSections + [appearanceSection]
    }

    private var selectedSection: SettingsSectionModel? {
        supportedSections.first { $0.id == selection }
    }

    private func selectDefaultSectionIfNeeded() {
        let sections = supportedSections
        guard !sections.isEmpty else {
            selection = nil
            return
        }
        if selection == nil || !sections.contains(where: { $0.id == selection }) {
            selection = sections[0].id
        }
    }
}

private struct SettingsSidebarView: View {
    let sections: [SettingsSectionModel]
    @Binding var selection: String?

    var body: some View {
        List(selection: $selection) {
            Section {
                ForEach(sections) { section in
                    SettingsSidebarRow(section: section)
                        .tag(section.id as String?)
                }
            }
        }
        .listStyle(.sidebar)
        .navigationSplitViewColumnWidth(min: 176, ideal: 192, max: 230)
    }
}

private struct SettingsSidebarRow: View {
    let section: SettingsSectionModel

    var body: some View {
        Label(section.label, systemImage: section.systemImage)
            .lineLimit(1)
    }
}

private struct SettingsDetailView: View {
    let section: SettingsSectionModel?
    @ObservedObject var store: SettingsStore

    var body: some View {
        Group {
            if let section {
                if section.id == SettingsRootView.appearanceSectionID {
                    LogoSettingsView()
                } else {
                    SettingsSectionView(section: section, store: store)
                }
            } else {
                ContentUnavailableView(
                    "Settings",
                    systemImage: "gearshape",
                    description: Text("No supported settings are available.")
                )
            }
        }
        .frame(minWidth: 500, maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct SettingsSectionView: View {
    let section: SettingsSectionModel
    @ObservedObject var store: SettingsStore

    var body: some View {
        Form {
            if let colors = section.colors {
                ColorSettingsContent(colors: colors, store: store)
            } else if section.keybinds != nil {
                KeybindSettingsContent(store: store)
            } else {
                ForEach(section.groups ?? []) { group in
                    Section(group.label) {
                        ForEach(group.settings) { setting in
                            SettingRow(setting: setting, store: store)
                        }
                    }
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(section.label)
    }
}

private extension SettingsSectionModel {
    var hasSupportedSettings: Bool {
        !(groups?.flatMap(\.settings).isEmpty ?? true)
            || !(colors?.isEmpty ?? true)
            || keybinds != nil
    }

    var supportedSettingCount: Int {
        if let colors {
            return colors.count
        }
        if keybinds != nil {
            return 1
        }
        return groups?.reduce(0) { count, group in
            count + group.settings.count
        } ?? 0
    }
}

// MARK: - Appearance / logo switcher

private struct LogoSettingsView: View {
    @ObservedObject private var logos = AppLogoManager.shared

    var body: some View {
        Form {
            Section("App Logo") {
                Picker(selection: $logos.selectedID) {
                    ForEach(AppLogo.all) { logo in
                        Text(logo.label).tag(logo.id)
                    }
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Logo")
                        Text("Sets the Dock and Cmd-Tab icon. Remembered across launches.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                LabeledContent("Preview") {
                    LogoPreview(image: logos.image(for: logos.selected))
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Appearance")
    }
}

private struct LogoPreview: View {
    let image: NSImage?

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFit()
                    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
            } else {
                RoundedRectangle(cornerRadius: 18, style: .continuous)
                    .fill(Color.secondary.opacity(0.15))
                    .overlay(
                        Image(systemName: "questionmark")
                            .foregroundStyle(.secondary)
                    )
            }
        }
        .frame(width: 96, height: 96)
    }
}

// MARK: - Generic root setting row

private struct SettingRow: View {
    let setting: Setting
    @ObservedObject var store: SettingsStore

    var body: some View {
        switch setting.kind {
        case .boolean:
            Toggle(isOn: store.boolBinding(setting.key)) {
                SettingLabelView(setting: setting)
            }
        case .enumeration:
            ChoiceSettingRow(setting: setting, store: store)
        case .numeric:
            NumericSettingRow(setting: setting, store: store)
        case .text:
            CommittingTextFieldRow(setting: setting, store: store, maxWidth: 240)
        case .special:
            if setting.choices?.isEmpty == false {
                ChoiceSettingRow(setting: setting, store: store)
            } else {
                CommittingTextFieldRow(setting: setting, store: store, maxWidth: 240)
            }
        }
    }
}

private struct ChoiceSettingRow: View {
    let setting: Setting
    @ObservedObject var store: SettingsStore

    var body: some View {
        Picker(selection: store.enumBinding(setting.key)) {
            ForEach(setting.choices ?? []) { choice in
                Text(choice.label).tag(choice.value)
            }
        } label: {
            SettingLabelView(setting: setting)
        }
    }
}

private struct SettingLabelView: View {
    let title: String
    let description: String

    init(setting: Setting) {
        title = setting.title
        description = setting.description
    }

    init(color: ColorSetting) {
        title = color.title
        description = color.description
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
            Text(description)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}

private struct NumericSettingRow: View {
    let setting: Setting
    @ObservedObject var store: SettingsStore

    var body: some View {
        if let range = Self.sliderRange(for: setting.key) {
            LabeledContent {
                HStack(spacing: 10) {
                    Slider(
                        value: Binding(
                            get: { Double(store.value(for: setting.key)) ?? range.lowerBound },
                            set: { store.commitRoot(key: setting.key, value: Self.format($0)) }
                        ),
                        in: range,
                        step: Self.step(for: setting.key)
                    )
                    .frame(width: 180)
                    Text(store.value(for: setting.key))
                        .font(.system(.body, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .frame(width: 48, alignment: .trailing)
                }
            } label: {
                SettingLabelView(setting: setting)
            }
        } else {
            CommittingTextFieldRow(setting: setting, store: store, maxWidth: 120)
        }
    }

    private static func sliderRange(for key: String) -> ClosedRange<Double>? {
        switch key {
        case "background_opacity":
            return 0...1
        case "pane_focus_strength":
            return 0...2
        case "line_height":
            return 0.8...2.5
        case "mouse_scroll_multiplier":
            return 0.1...10
        default:
            return nil
        }
    }

    private static func step(for key: String) -> Double {
        key == "mouse_scroll_multiplier" ? 0.1 : 0.05
    }

    private static func format(_ value: Double) -> String {
        String(format: "%.2f", value)
    }
}

private struct CommittingTextFieldRow: View {
    let setting: Setting
    @ObservedObject var store: SettingsStore
    let maxWidth: CGFloat

    @State private var text: String = ""
    @FocusState private var focused: Bool

    var body: some View {
        LabeledContent {
            TextField(setting.title, text: $text)
                .textFieldStyle(.roundedBorder)
                .frame(maxWidth: maxWidth)
                .focused($focused)
                .onSubmit(commit)
                .onChange(of: focused) { _, isFocused in
                    if !isFocused {
                        commit()
                    }
                }
        } label: {
            SettingLabelView(setting: setting)
        }
        .onAppear {
            text = store.value(for: setting.key)
        }
        .onChange(of: store.value(for: setting.key)) { _, newValue in
            if !focused {
                text = newValue
            }
        }
    }

    private func commit() {
        store.commitRoot(key: setting.key, value: text)
    }
}

// MARK: - Colors

private struct ColorSettingsContent: View {
    let colors: [ColorSetting]
    @ObservedObject var store: SettingsStore

    var body: some View {
        Section("Base") {
            ForEach(colors.prefix(3)) { color in
                ColorRow(color: color, store: store)
            }
        }
        Section("ANSI Palette") {
            ForEach(colors.dropFirst(3)) { color in
                ColorRow(color: color, store: store)
            }
        }
    }
}

private struct ColorRow: View {
    let color: ColorSetting
    @ObservedObject var store: SettingsStore

    var body: some View {
        LabeledContent {
            HStack(spacing: 10) {
                ColorPicker("", selection: pickerBinding, supportsOpacity: false)
                    .labelsHidden()
                if !store.colorHex(for: color.key).isEmpty {
                    Button("Reset") {
                        store.commitColor(key: color.key, hex: nil)
                    }
                    .buttonStyle(.borderless)
                    .font(.caption)
                } else {
                    Text("theme")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } label: {
            SettingLabelView(color: color)
        }
    }

    private var pickerBinding: Binding<Color> {
        Binding(
            get: { Color(hex: store.colorHex(for: color.key)) ?? Color(white: 0.5) },
            set: { newColor in
                if let hex = newColor.hexString {
                    store.commitColor(key: color.key, hex: hex)
                }
            }
        )
    }
}

// MARK: - Keybindings

private struct KeybindSettingsContent: View {
    @ObservedObject var store: SettingsStore

    var body: some View {
        Section("Keybind Directives") {
            Text("One directive per line, e.g. `cmd-k=clear_buffer`. These are written verbatim as `keybind = …` lines.")
                .font(.caption)
                .foregroundStyle(.secondary)

            TextEditor(text: $store.keybindsText)
                .font(.system(.body, design: .monospaced))
                .frame(minHeight: 220)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(Color.secondary.opacity(0.25), lineWidth: 1)
                )

            HStack {
                Spacer()
                Button("Apply Keybinds") {
                    store.commitKeybinds()
                }
                .keyboardShortcut("s", modifiers: [.command])
            }
        }
    }
}

// MARK: - Color hex helpers

extension Color {
    init?(hex: String) {
        guard let rgb = RGBHexColor(hex: hex) else {
            return nil
        }
        self = Color(
            red: rgb.red,
            green: rgb.green,
            blue: rgb.blue
        )
    }

    var hexString: String? {
        guard let srgb = NSColor(self).usingColorSpace(.sRGB) else {
            return nil
        }
        let r = Int((srgb.redComponent * 255).rounded())
        let g = Int((srgb.greenComponent * 255).rounded())
        let b = Int((srgb.blueComponent * 255).rounded())
        return String(format: "#%02x%02x%02x", r, g, b)
    }
}

private struct RGBHexColor {
    var red: Double
    var green: Double
    var blue: Double

    init?(hex: String) {
        var value = hex.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else {
            return nil
        }
        if value.hasPrefix("#") {
            value.removeFirst()
        }
        guard value.count == 6, let packed = Int(value, radix: 16) else {
            return nil
        }
        red = Self.component(packed >> 16)
        green = Self.component(packed >> 8)
        blue = Self.component(packed)
    }

    private static func component(_ packedComponent: Int) -> Double {
        Double(packedComponent & 0xff) / 255.0
    }
}

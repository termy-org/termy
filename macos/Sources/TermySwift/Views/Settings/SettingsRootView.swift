import AppKit
import SwiftUI

struct SettingsRootView: View {
    @StateObject private var store = SettingsStore()
    @State private var selection: String?

    var body: some View {
        NavigationSplitView {
            List(selection: $selection) {
                ForEach(store.schema?.sections ?? []) { section in
                    Label(section.label, systemImage: section.systemImage)
                        .tag(section.id as String?)
                }
            }
            .navigationSplitViewColumnWidth(min: 200, ideal: 214, max: 260)
            .listStyle(.sidebar)
            .safeAreaInset(edge: .bottom) {
                HStack(alignment: .top, spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.yellow)
                    Text("Some settings may require a new terminal or app restart to take effect.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(.bar)
            }
        } detail: {
            Group {
                if let section = store.section(id: selection) {
                    SettingsSectionView(section: section, store: store)
                } else {
                    ContentUnavailableView(
                        "Settings",
                        systemImage: "gearshape",
                        description: Text("Select a category.")
                    )
                }
            }
            .frame(minWidth: 480, maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(minWidth: 760, minHeight: 520)
        .onAppear {
            store.load()
            if selection == nil {
                selection = store.schema?.sections.first?.id
            }
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

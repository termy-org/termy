import AppKit
import Combine

/// A selectable Dock / Cmd-Tab app icon. The `resourceName` is the PNG basename
/// bundled into the app's `Contents/Resources` by `script/build_and_run.sh`.
struct AppLogo: Identifiable, Hashable {
    let id: String
    let label: String
    let resourceName: String

    static let all: [AppLogo] = [
        AppLogo(id: "termy", label: "Termy Icon", resourceName: "TermyIcon"),
        AppLogo(id: "classic", label: "Classic", resourceName: "termy_old_icon"),
    ]

    static let `default` = all[1]
}

/// Owns the currently selected app logo from shared config and pushes it to the
/// live Dock icon (`NSApp.applicationIconImage`).
@MainActor
final class AppLogoManager: ObservableObject {
    static let shared = AppLogoManager()

    @Published private(set) var selectedID: String
    private var imageCache: [String: NSImage] = [:]

    private init() {
        selectedID = Self.logoID(for: TermyConfigurationStore.shared.configuration.native.appIcon)
    }

    var selected: AppLogo {
        AppLogo.all.first { $0.id == selectedID } ?? .default
    }

    /// Loads a logo image from the app bundle's Resources.
    func image(for logo: AppLogo) -> NSImage? {
        if let cached = imageCache[logo.id] {
            return cached
        }
        let image: NSImage?
        if let pngURL = Bundle.main.url(forResource: logo.resourceName, withExtension: "png") {
            image = NSImage(contentsOf: pngURL)
        } else if let icnsURL = Bundle.main.url(forResource: logo.resourceName, withExtension: "icns") {
            image = NSImage(contentsOf: icnsURL)
        } else {
            image = nil
        }
        if let image {
            imageCache[logo.id] = image
        }
        return image
    }

    /// Applies the selected logo to the running app's Dock / Cmd-Tab icon.
    /// Called on launch and whenever the selection changes.
    func applyToDock() {
        if let image = image(for: selected) {
            NSApp.applicationIconImage = image
        }
    }

    func reloadFromConfig() {
        let nextID = Self.logoID(for: TermyConfigurationStore.shared.reload().native.appIcon)
        guard nextID != selectedID else {
            return
        }
        selectedID = nextID
        applyToDock()
    }

    private static func logoID(for appIcon: TermyAppIcon) -> String {
        switch appIcon {
        case .default:
            return "termy"
        case .old:
            return "classic"
        }
    }
}

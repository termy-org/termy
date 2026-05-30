import AppKit
import Combine

/// A selectable Dock / Cmd-Tab app icon. The `resourceName` is the PNG basename
/// bundled into the app's `Contents/Resources` by `script/build_and_run.sh`.
struct AppLogo: Identifiable, Hashable {
    let id: String
    let label: String
    let resourceName: String

    static let all: [AppLogo] = [
        AppLogo(id: "toyko", label: "ToykoTermy", resourceName: "ToykoTermy"),
        AppLogo(id: "classic", label: "Classic", resourceName: "termy_old_icon"),
        AppLogo(id: "termy", label: "Termy Icon", resourceName: "TermyIcon"),
    ]

    /// ToykoTermy is the default logo.
    static let `default` = all[0]
}

/// Owns the currently selected app logo, persists it across launches, and pushes
/// it to the live Dock icon (`NSApp.applicationIconImage`).
@MainActor
final class AppLogoManager: ObservableObject {
    static let shared = AppLogoManager()

    private let defaultsKey = "selectedAppLogoID"

    /// Setting this persists the choice and updates the Dock icon immediately.
    @Published var selectedID: String {
        didSet {
            guard oldValue != selectedID else { return }
            UserDefaults.standard.set(selectedID, forKey: defaultsKey)
            applyToDock()
        }
    }

    private init() {
        let stored = UserDefaults.standard.string(forKey: defaultsKey)
        selectedID = AppLogo.all.contains { $0.id == stored } ? stored! : AppLogo.default.id
    }

    var selected: AppLogo {
        AppLogo.all.first { $0.id == selectedID } ?? .default
    }

    /// Loads a logo image from the app bundle's Resources.
    func image(for logo: AppLogo) -> NSImage? {
        guard let url = Bundle.main.url(forResource: logo.resourceName, withExtension: "png") else {
            return nil
        }
        return NSImage(contentsOf: url)
    }

    /// Applies the selected logo to the running app's Dock / Cmd-Tab icon.
    /// Called on launch and whenever the selection changes.
    func applyToDock() {
        if let image = image(for: selected) {
            NSApp.applicationIconImage = image
        }
    }
}

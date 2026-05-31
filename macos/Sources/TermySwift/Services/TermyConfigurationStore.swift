import Combine
import Foundation

@MainActor
final class TermyConfigurationStore: ObservableObject {
    static let shared = TermyConfigurationStore()

    @Published private(set) var configuration: TermyAppConfiguration
    @Published private(set) var loadErrorMessage: String?

    private var settingsObserver: NSObjectProtocol?

    private init() {
        configuration = TermyAppConfiguration.current
        loadErrorMessage = TermyAppConfiguration.loadErrorMessage
        settingsObserver = NotificationCenter.default.addObserver(
            forName: .termySettingsChanged,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.reload()
            }
        }
    }

    @discardableResult
    func reload() -> TermyAppConfiguration {
        do {
            let configuration = try TermyAppConfiguration.loadFresh()
            self.configuration = configuration
            loadErrorMessage = nil
            return configuration
        } catch {
            loadErrorMessage = String(describing: error)
            return configuration
        }
    }
}

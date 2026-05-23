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

    var windowSize: CGSize {
        CGSize(width: windowWidth, height: windowHeight)
    }

    private static let defaultConfiguration = TermyAppConfiguration(
        windowWidth: 1280,
        windowHeight: 820
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

        return TermyAppConfiguration(
            windowWidth: CGFloat(max(320, width)),
            windowHeight: CGFloat(max(240, height))
        )
    }

}

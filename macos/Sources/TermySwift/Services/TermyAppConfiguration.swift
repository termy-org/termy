import CTermy
import CoreGraphics
import Foundation

struct TermyAppConfiguration {
    var windowWidth: CGFloat
    var windowHeight: CGFloat
    var workingDirectory: String?

    var windowSize: CGSize {
        CGSize(width: windowWidth, height: windowHeight)
    }

    static let current = load()

    static let fallback = TermyAppConfiguration(
        windowWidth: 1280,
        windowHeight: 820,
        workingDirectory: nil
    )

    private static func load() -> TermyAppConfiguration {
        var config: OpaquePointer?
        guard termy_config_load_default(&config) == TERMY_FFI_OK, let config else {
            return .fallback
        }
        defer {
            _ = termy_config_free(config)
        }

        var width: Float = Float(fallback.windowWidth)
        var height: Float = Float(fallback.windowHeight)
        _ = termy_config_window_size(config, &width, &height)

        return TermyAppConfiguration(
            windowWidth: CGFloat(max(320, width)),
            windowHeight: CGFloat(max(240, height)),
            workingDirectory: workingDirectory(from: config)
        )
    }

    private static func workingDirectory(from config: OpaquePointer) -> String? {
        var bytes = TermyFfiBytes()
        guard termy_config_working_directory(config, &bytes) == TERMY_FFI_OK else {
            return nil
        }
        defer {
            if bytes.ptr != nil {
                _ = termy_buffer_free(bytes)
            }
        }

        guard let ptr = bytes.ptr, bytes.len > 0 else {
            return nil
        }
        let buffer = UnsafeBufferPointer(start: ptr, count: Int(bytes.len))
        let value = String(decoding: buffer, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}

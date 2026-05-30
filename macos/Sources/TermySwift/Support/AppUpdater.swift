import AppKit

/// Checks GitHub Releases for a newer version and guides the user to download
/// it. (Foundational: it surfaces updates and opens the release page rather than
/// installing silently, which would require a Sparkle-style framework.)
@MainActor
final class AppUpdater {
    static let shared = AppUpdater()

    private let endpoint = URL(
        string: "https://api.github.com/repos/lassejlv/termy/releases/latest"
    )!
    private var isChecking = false

    struct Release {
        let version: String
        let url: URL
    }

    enum UpdateError: Error {
        case invalidResponse
    }

    func checkForUpdates(userInitiated: Bool) async {
        guard !isChecking else {
            return
        }
        isChecking = true
        defer { isChecking = false }

        do {
            let release = try await fetchLatest()
            if isNewer(release.version, than: currentVersion()) {
                presentUpdateAvailable(release)
            } else if userInitiated {
                presentUpToDate()
            }
        } catch {
            if userInitiated {
                presentError(error)
            }
        }
    }

    private func fetchLatest() async throws -> Release {
        var request = URLRequest(url: endpoint)
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        let (data, _) = try await URLSession.shared.data(for: request)
        let decoded = try JSONDecoder().decode(GitHubRelease.self, from: data)
        let version = decoded.tagName.trimmingCharacters(in: CharacterSet(charactersIn: "vV"))
        guard let url = URL(string: decoded.htmlURL) else {
            throw UpdateError.invalidResponse
        }
        return Release(version: version, url: url)
    }

    private func currentVersion() -> String {
        (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "0.0.0"
    }

    /// Numeric component-wise semver comparison (ignores pre-release tags).
    private func isNewer(_ candidate: String, than current: String) -> Bool {
        func components(_ value: String) -> [Int] {
            value
                .split(whereSeparator: { $0 == "." || $0 == "-" })
                .map { Int($0) ?? 0 }
        }
        let lhs = components(candidate)
        let rhs = components(current)
        for index in 0..<max(lhs.count, rhs.count) {
            let left = index < lhs.count ? lhs[index] : 0
            let right = index < rhs.count ? rhs[index] : 0
            if left != right {
                return left > right
            }
        }
        return false
    }

    private func presentUpdateAvailable(_ release: Release) {
        let alert = NSAlert()
        alert.messageText = "Update available"
        alert.informativeText = "Termy \(release.version) is available. You're on \(currentVersion())."
        alert.addButton(withTitle: "Download")
        alert.addButton(withTitle: "Later")
        if alert.runModal() == .alertFirstButtonReturn {
            NSWorkspace.shared.open(release.url)
        }
    }

    private func presentUpToDate() {
        let alert = NSAlert()
        alert.messageText = "You're up to date"
        alert.informativeText = "Termy \(currentVersion()) is the latest version."
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    private func presentError(_ error: Error) {
        let alert = NSAlert()
        alert.messageText = "Couldn't check for updates"
        alert.informativeText = String(describing: error)
        alert.alertStyle = .warning
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }
}

private struct GitHubRelease: Decodable {
    let tagName: String
    let htmlURL: String

    enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"
        case htmlURL = "html_url"
    }
}

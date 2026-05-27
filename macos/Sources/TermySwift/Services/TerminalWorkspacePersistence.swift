import Foundation

enum TerminalWorkspacePersistenceError: Error, CustomStringConvertible {
    case missingLastSession

    var description: String {
        switch self {
        case .missingLastSession:
            return "No persisted terminal workspace session exists"
        }
    }
}

struct TerminalWorkspacePersistenceState: Codable, Equatable {
    static let currentVersion = 1

    var version = Self.currentVersion
    var lastSession: TerminalWorkspaceSnapshot?
    var layouts: [TerminalWorkspaceNamedLayout] = []

    enum CodingKeys: String, CodingKey {
        case version
        case lastSession = "last_session"
        case layouts
    }
}

struct TerminalWorkspaceNamedLayout: Codable, Equatable {
    var name: String
    var workspace: TerminalWorkspaceSnapshot
}

struct TerminalWorkspaceSnapshot: Codable, Equatable {
    var activeTab = 0
    var tabs: [TerminalWorkspaceTabSnapshot]

    enum CodingKeys: String, CodingKey {
        case activeTab = "active_tab"
        case tabs
    }
}

struct TerminalWorkspaceTabSnapshot: Codable, Equatable {
    var panes: [TerminalWorkspacePaneSnapshot]
    var layoutTree: TerminalWorkspaceLayoutNode?
    var activePane: Int
    var isSearchVisible: Bool
    var pinned = false
    var manualTitle: String?

    enum CodingKeys: String, CodingKey {
        case panes
        case layoutTree = "layout_tree"
        case activePane = "active_pane"
        case isSearchVisible = "is_search_visible"
        case pinned
        case manualTitle = "manual_title"
    }
}

struct TerminalWorkspacePaneSnapshot: Codable, Equatable {
    var id: UUID
    var title: String?
}

indirect enum TerminalWorkspaceLayoutNode: Codable, Equatable {
    case leaf(pane: Int)
    case split(axis: TerminalSplitAxis, ratio: Double, first: TerminalWorkspaceLayoutNode, second: TerminalWorkspaceLayoutNode)

    private enum CodingKeys: String, CodingKey {
        case kind
        case pane
        case axis
        case ratio
        case first
        case second
    }

    private enum Kind: String, Codable {
        case leaf
        case split
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(Kind.self, forKey: .kind)
        switch kind {
        case .leaf:
            self = .leaf(pane: try container.decode(Int.self, forKey: .pane))
        case .split:
            let axis = try container.decode(TerminalSplitAxis.self, forKey: .axis)
            let ratio = try container.decode(Double.self, forKey: .ratio)
            self = .split(
                axis: axis,
                ratio: ratio,
                first: try container.decode(TerminalWorkspaceLayoutNode.self, forKey: .first),
                second: try container.decode(TerminalWorkspaceLayoutNode.self, forKey: .second)
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .leaf(let pane):
            try container.encode(Kind.leaf, forKey: .kind)
            try container.encode(pane, forKey: .pane)
        case .split(let axis, let ratio, let first, let second):
            try container.encode(Kind.split, forKey: .kind)
            try container.encode(axis, forKey: .axis)
            try container.encode(ratio, forKey: .ratio)
            try container.encode(first, forKey: .first)
            try container.encode(second, forKey: .second)
        }
    }
}

extension TerminalSplitAxis: Codable {
    init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer().decode(String.self)
        switch value {
        case "horizontal":
            self = .horizontal
        case "vertical":
            self = .vertical
        default:
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "Invalid terminal split axis '\(value)'")
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .horizontal:
            try container.encode("horizontal")
        case .vertical:
            try container.encode("vertical")
        }
    }
}

struct TerminalWorkspacePersistence {
    var fileURL: URL

    init(fileURL: URL = Self.defaultFileURL()) {
        self.fileURL = fileURL
    }

    static func defaultFileURL(
        fileManager: FileManager = .default,
        bundleIdentifier: String = Bundle.main.bundleIdentifier ?? "com.lassevestergaard.TermyAlpha"
    ) -> URL {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? fileManager.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support", isDirectory: true)
        return appSupport
            .appendingPathComponent(bundleIdentifier, isDirectory: true)
            .appendingPathComponent("native-workspace.json", isDirectory: false)
    }

    func loadState() throws -> TerminalWorkspacePersistenceState {
        do {
            let data = try Data(contentsOf: fileURL)
            return try JSONDecoder().decode(TerminalWorkspacePersistenceState.self, from: data)
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
            return TerminalWorkspacePersistenceState()
        } catch let error as DecodingError {
            throw error
        } catch {
            let nsError = error as NSError
            if nsError.domain == NSCocoaErrorDomain && nsError.code == NSFileReadNoSuchFileError {
                return TerminalWorkspacePersistenceState()
            }
            throw error
        }
    }

    func loadLastSession() throws -> TerminalWorkspaceSnapshot {
        guard let lastSession = try loadState().lastSession else {
            throw TerminalWorkspacePersistenceError.missingLastSession
        }
        return lastSession
    }

    func saveLastSession(_ snapshot: TerminalWorkspaceSnapshot?) throws {
        var state = try loadState()
        state.lastSession = snapshot
        try saveState(state)
    }

    func clearLastSession() throws {
        try saveLastSession(nil)
    }

    func saveState(_ state: TerminalWorkspacePersistenceState) throws {
        if state.lastSession == nil && state.layouts.isEmpty {
            try? FileManager.default.removeItem(at: fileURL)
            return
        }

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(state)
        let directory = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)

        let temporaryURL = directory.appendingPathComponent(".\(fileURL.lastPathComponent).tmp")
        try data.write(to: temporaryURL, options: .atomic)
        if FileManager.default.fileExists(atPath: fileURL.path) {
            _ = try FileManager.default.replaceItemAt(fileURL, withItemAt: temporaryURL)
        } else {
            try FileManager.default.moveItem(at: temporaryURL, to: fileURL)
        }
    }
}

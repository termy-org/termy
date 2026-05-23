import AppKit

enum TerminalDropInput {
    static let acceptedPasteboardTypes: [NSPasteboard.PasteboardType] = [
        .fileURL,
        .URL,
        .tiff,
        NSPasteboard.PasteboardType("public.file-url"),
        NSPasteboard.PasteboardType("public.png"),
        NSPasteboard.PasteboardType("public.jpeg"),
        NSPasteboard.PasteboardType("public.tiff"),
        NSPasteboard.PasteboardType("public.webarchive")
    ]

    static func bytes(from pasteboard: NSPasteboard) -> [UInt8]? {
        if let urls = fileURLs(from: pasteboard), !urls.isEmpty {
            return shellQuotedBytes(for: urls.map(\.path))
        }

        if let imagePath = writeImagePasteboardItemToTemporaryFile(pasteboard) {
            return shellQuotedBytes(for: [imagePath.path])
        }

        return nil
    }

    static func canDecode(_ pasteboard: NSPasteboard) -> Bool {
        if let urls = fileURLs(from: pasteboard), !urls.isEmpty {
            return true
        }
        if imageCandidates.contains(where: { pasteboard.data(forType: $0.type) != nil }) {
            return true
        }
        return NSImage(pasteboard: pasteboard) != nil
    }

    private static func fileURLs(from pasteboard: NSPasteboard) -> [URL]? {
        if let urls = pasteboard.readObjects(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ) as? [URL], !urls.isEmpty {
            return urls
        }

        let urlTypes: [NSPasteboard.PasteboardType] = [
            .fileURL,
            NSPasteboard.PasteboardType("public.file-url")
        ]
        let urls = urlTypes.compactMap { type -> URL? in
            guard let value = pasteboard.string(forType: type) else {
                return nil
            }
            guard let url = URL(string: value), url.isFileURL else {
                return nil
            }
            return url
        }

        return urls.isEmpty ? nil : urls
    }

    private static func shellQuotedBytes(for paths: [String]) -> [UInt8]? {
        guard !paths.isEmpty else {
            return nil
        }

        let text = paths
            .map(shellQuotePath)
            .joined(separator: " ") + " "
        return Array(text.utf8)
    }

    private static func shellQuotePath(_ path: String) -> String {
        "'\(path.replacingOccurrences(of: "'", with: "'\\''"))'"
    }

    private static func writeImagePasteboardItemToTemporaryFile(_ pasteboard: NSPasteboard) -> URL? {
        for candidate in imageCandidates {
            guard let data = pasteboard.data(forType: candidate.type) else {
                continue
            }
            return writeImageData(data, fileExtension: candidate.extension)
        }

        guard let image = NSImage(pasteboard: pasteboard),
              let data = image.tiffRepresentation
        else {
            return nil
        }
        return writeImageData(data, fileExtension: "tiff")
    }

    private static let imageCandidates: [(type: NSPasteboard.PasteboardType, extension: String)] = [
        (NSPasteboard.PasteboardType("public.png"), "png"),
        (NSPasteboard.PasteboardType("public.jpeg"), "jpg"),
        (.tiff, "tiff")
    ]

    private static func writeImageData(_ data: Data, fileExtension: String) -> URL? {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("termy-dropped-images", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let fileURL = directory.appendingPathComponent("dropped-image-\(UUID().uuidString).\(fileExtension)")
            try data.write(to: fileURL, options: .atomic)
            return fileURL
        } catch {
            return nil
        }
    }
}

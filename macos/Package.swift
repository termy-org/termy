// swift-tools-version: 6.0

import Foundation
import PackageDescription

let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repoRoot = packageDirectory.deletingLastPathComponent()
let ffiLibraryPath = ProcessInfo.processInfo.environment["TERMY_FFI_LIBRARY_PATH"]
    ?? repoRoot.appendingPathComponent("target/debug").path

let package = Package(
    name: "termy-swift",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "TermySwift", targets: ["TermySwift"])
    ],
    targets: [
        .systemLibrary(
            name: "CTermy",
            path: "Sources/CTermy"
        ),
        .executableTarget(
            name: "TermySwift",
            dependencies: ["CTermy"],
            linkerSettings: [
                .unsafeFlags([
                    "-L", ffiLibraryPath,
                    "-ltermy_ffi",
                    "-Xlinker", "-rpath",
                    "-Xlinker", "@executable_path/../Frameworks",
                    "-Xlinker", "-rpath",
                    "-Xlinker", ffiLibraryPath
                ])
            ]
        )
    ]
)

// swift-tools-version: 6.0

import Foundation
import PackageDescription

let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repoRoot = packageDirectory
    .deletingLastPathComponent()
    .deletingLastPathComponent()
let ffiLibraryPath = repoRoot.appendingPathComponent("target/debug").path

let package = Package(
    name: "libtermy-swift-example",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(
            name: "libtermy-swift-example",
            targets: ["LibTermySwiftExample"]
        )
    ],
    targets: [
        .systemLibrary(
            name: "CTermy",
            path: "Sources/CTermy"
        ),
        .executableTarget(
            name: "LibTermySwiftExample",
            dependencies: ["CTermy"],
            linkerSettings: [
                .unsafeFlags([
                    "-L", ffiLibraryPath,
                    "-ltermy_ffi",
                    "-Xlinker", "-rpath",
                    "-Xlinker", ffiLibraryPath,
                ])
            ]
        ),
    ]
)

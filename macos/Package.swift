// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "Warlogger",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(name: "Warlogger", path: "Sources/Warlogger")
    ],
    swiftLanguageModes: [.v5]
)

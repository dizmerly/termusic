// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "TermusicBar",
    platforms: [.macOS(.v13)],
    targets: [
        .target(name: "TermusicProtocol"),
        .executableTarget(name: "TermusicBar", dependencies: ["TermusicProtocol"]),
        .testTarget(name: "TermusicProtocolTests", dependencies: ["TermusicProtocol"]),
    ]
)

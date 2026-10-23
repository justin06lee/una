// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "UnaKit",
    platforms: [.iOS(.v18), .macOS(.v14)],
    products: [.library(name: "UnaKit", targets: ["UnaKit"])],
    targets: [
        .target(name: "UnaKit"),
        .testTarget(name: "UnaKitTests", dependencies: ["UnaKit"], resources: [.copy("Fixtures")]),
    ]
)

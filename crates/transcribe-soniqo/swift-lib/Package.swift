// swift-tools-version:5.9

import PackageDescription

let package = Package(
  name: "soniqo-swift",
  // Matches `apps/desktop/src-tauri/tauri.conf.json` -> minimumSystemVersion "15.0".
  // Every speech-swift ASR product already declares macOS 15, and Qwen3ASR's CoreML
  // decoder uses MLState (macOS 15+), so the old 14.2 floor was stale, not a guarantee.
  platforms: [.macOS("15.0")],
  products: [
    .library(
      name: "soniqo-swift",
      type: .static,
      targets: ["swift-lib"])
  ],
  dependencies: [
    .package(
      url: "https://github.com/Brendonovich/swift-rs",
      revision: "01980f981bc642a6da382cc0788f18fdd4cde6df"),
    .package(url: "https://github.com/soniqo/speech-swift", exact: "0.0.22"),
  ],
  targets: [
    .target(
      name: "swift-lib",
      dependencies: [
        .product(name: "AudioCommon", package: "speech-swift"),
        .product(name: "OmnilingualASR", package: "speech-swift"),
        .product(name: "ParakeetASR", package: "speech-swift"),
        .product(name: "ParakeetStreamingASR", package: "speech-swift"),
        .product(name: "Qwen3ASR", package: "speech-swift"),
        .product(name: "SpeechVAD", package: "speech-swift"),
        .product(name: "SwiftRs", package: "swift-rs"),
      ],
      path: "src"
    )
  ]
)

# Rust Dev Client

An example of building a cross-platform, GUI application using the [LiveKit Rust SDK](https://github.com/livekit/rust-sdks).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="resources/screenshots/connect-dark.png">
  <img alt="Connect window" src="resources/screenshots/connect-light.png" width="350">
</picture>

## Features

- [x] Platform support: macOS, Linux, and Windows
- [x] Connect to multiple LiveKit rooms
- [x] Use either pre-generated token or project API key/secret
- [x] Publish test tracks
- [x] Subscribe to tracks
- [x] Simulate fault scenarios (e.g., reconnect, migration, etc.)
- [x] Send/receive remote procedure calls (RPC)
- [x] Send/receive data streams

## Building

The app builds and runs against the pinned, published LiveKit crates by default:

```sh
cargo run              # debug
cargo run --release    # optimized
```

The Rust toolchain is pinned in `rust-toolchain.toml` and selected automatically. The first build downloads a prebuilt `libwebrtc`, so it needs network access.

### Building against a local rust-sdks

To build against a local checkout of the [LiveKit Rust SDK](https://github.com/livekit/rust-sdks) instead of the published crates:

1. Clone the SDK next to this repository:
   ```sh
   git clone https://github.com/livekit/rust-sdks ../rust-sdks
   ```
2. Uncomment the `[patch.crates-io]` block in `Cargo.toml` and comment in the upper appearance of `livekit` and `livekit-api` (edit the paths if your checkout lives elsewhere):
   ```toml
   [patch.crates-io]
   livekit = { path = "../rust-sdks/livekit" }
   livekit-api = { path = "../rust-sdks/livekit-api" }
   ```
3. If the checkout's crate version differs from `Cargo.lock`, bind it once so the patch takes effect — otherwise cargo prints `warning: patch ... was not used in the crate graph` and keeps the published version:
   ```sh
   cargo update -p livekit -p livekit-api
   ```
4. `cargo run` — and rust-analyzer — now build against the local sources.

To return to the published crates, re-comment the block and run `git checkout Cargo.lock`.

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
2. In `Cargo.toml`, comment out the two published `livekit` / `livekit-api` lines and uncomment the `path` lines below them (edit the paths if your checkout lives elsewhere):
   ```toml
   livekit = { path = "../rust-sdks/livekit", features = ["rustls-tls-native-roots"] }
   livekit-api = { path = "../rust-sdks/livekit-api", default-features = false, features = ["access-token"] }
   ```
   These are plain `path` dependencies, so cargo re-resolves automatically — no `cargo update` needed.
3. `cargo run` — and rust-analyzer — now build against the local sources.

To return to the published crates, reverse step 2: uncomment the two version lines and comment the `path` lines back out.

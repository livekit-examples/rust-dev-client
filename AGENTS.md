# AGENTS.md

## Background

- This application was previously called the `wgpu_room` exampled and lived in the [main repo](https://github.com/livekit/rust-sdks)
- It has been moved here to become a refined, standalone developer tool
- Existing code may not adhere to the requirements defined herein, however, new code and refactors should

## Building

- `cargo run` builds and runs the app against the pinned, published LiveKit crates
- To build against a local `rust-sdks` checkout, uncomment the `[patch.crates-io]` block in _Cargo.toml_ (see "Building against a local rust-sdks" in _README.md_)

## UI best practices

- This application uses [egui](https://docs.rs/egui/latest/egui/)
- Always reference egui docs before writing UI code
  - The [example collection](https://github.com/emilk/egui/tree/main/examples) is an especially important resource
  - Leverage built in functionality before reaching for custom solutions
- Avoid expensive computation in the UI path (egui is immediate mode)
- Never block the UI path (no `block_on`, blocking `recv`, or I/O); hand work to an async actor and deliver results back as events
- Do not hold mutex guards across widget closures; lock, copy out what the frame needs, drop the guard
- Keep application state outside the UI code, UI is a function of state
  - Separate domain model from UI selection/editing state
- Use `ScrollArea` and virtualization-like patterns for large lists
- Use stable, explicit IDs keyed by domain identity (e.g., track SID), never loop index or display name
- Reuse `TextureHandle`s/wgpu textures across frames; reallocate only on size change and release them when the track ends
- Pull colors/spacing from `src/style` and egui's `Style` rather than hardcoding values inline
- Break out custom widgets when appropriate to keep codebase DRY
  - Criteria: can be easily decoupled from its environment and is potentially useful in other contexts
  - Custom widgets implement the `egui::Widget` trait
  - Each custom widget belongs in its own module under `src/ui`

## Design patterns & conventions

- Generally avoid clones, but pay special attention when cloning in a high-frequency code path
  - In such cases, reach for smart pointers (e.g., `Arc<T>`) instead
- Implement `From`/`TryFrom` for performing conversion between types
- Prefer the actor pattern for async tasks
  - Model as a struct encapsulating local state with an async, consuming run method
  - Other methods can operate on `&self` to keep `run` small

## Safety

- Avoid `unwrap` except in tests
  - When unavoidable, prefer `expect` instead and provide a concise message explaining what went wrong (e.g., "Invalid state")
- Avoid `unsafe` unless absolutely necessary
- When unavoidable, follow these guidelines
  - Wrap unsafe code in a safe function or struct
  - Isolate only the unsafe operations
  - Every unsafe block should have a `// SAFETY:` comment explaining why the operation is actually safe (e.g., verifying pointers are non-null)

## Style guidelines

- Only add comments when doing so genuinely points out something non-obvious
  - Brevity is always a must
- Avoid excessive nesting and prefer [`let-else`](https://doc.rust-lang.org/rust-by-example/flow_control/let_else.html)
- Avoid long parameter lists; group related inputs into a purpose-built struct when it improves readability

## Contributing

- Adhere to requirements in [_CONTRIBUTING.md_](./CONTRIBUTING.md)
- Always format using `cargo fmt`
- Always address all issues, both clippy and compiler warnings
  - Do not reach for `#[allow(...)]` to bypass warnings unless it is unavoidable in the context
  - Be explicit when you are bypassing warnings
- Always run cspell and fix spelling issues
  - If a flagged word is valid project terminology, add it to _cspell.yml_ and sort the list alphabetically

## Release process

- There is currently no automatic release process in place
- To create a release
  - Bump version in _Cargo.toml_ through a PR (e.g., Release v0.1.0)
  - Create version tag (e.g., v0.1.0)
  - Publish GitHub release with automatically generated changelog
- Eventually this will be automated and builds for each platform will be attached to the release

## Documentation

- Attach tooltips to UI elements where doing so clarifies usage
- As new features are added, list them concisely in the features list in _README.md_

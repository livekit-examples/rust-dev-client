# AGENTS.md

## Background

- This application was previously called the `wgpu_room` exampled and lived in the [main repo](https://github.com/livekit/rust-sdks)
- It has been moved here to become a refined, standalone developer tool
- Existing code may not adhere to the requirements defined herein, however, new code and refactors should

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

## Merge requirements

- Always format using `cargo fmt`
- Alway run `cargo clippy` and fix issues
  - Do not reach for `#[allow(...)]` to bypass lint unless it is unavoidable in the context
  - Be explicit when you are bypassing lints

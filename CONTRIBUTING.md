# Contributing

Thanks for your interest in contributing!

## Before writing code

If you'd like to contribute code, it's recommended to first discuss your idea on the
[LiveKit Developer Community](https://community.livekit.io/c/robotics). This helps keep changes aligned with the LiveKit roadmap
and avoids duplicated or unnecessary work.

## Before you open a pull request

Please make sure your change passes all of the following:

```sh
cargo fmt --all -- --check                  # formatting
cargo clippy --all-targets -- -D warnings   # lints
cargo test                                  # tests
npx cspell .                                # spelling (must report no errors)
cargo run                                   # app still launches and behaves
```

- Add tests for non-UI logic (state, parsing, computations) where practical
- Since egui is immediate-mode and UI is hard to test automatically, manually verify the UI for anything that
touches the interface, and note what you checked in the PR
- Keep commits focused; a single logical change per PR is easiest to review

## Pull requests

- Open PRs against the `main` branch
- Describe *what* the change does and *why*
- For UI changes, a screenshot or short screen recording is very helpful
- Link any related issues (e.g. `Closes #123`)

## Reporting bugs

The issue tracker is for bugs or suspected bugs only. Bug reports must use the "Bug Report" template; issues that don't will be closed automatically.

## License

By contributing, you agree that your contributions will be licensed under the same terms as this project (see the `LICENSE` file)
# Contributing

Bug reports, platform notes and pull requests are welcome. For anything larger than a small
fix, open an issue first so the approach can be agreed before you spend time on it.

## Layout

| Crate | Contents |
| --- | --- |
| `crates/padless-core` | Key model, code tables, alias resolution, configuration and the input state machine. No OS dependencies and no `unsafe`. |
| `crates/padless-platform` | The `KeyboardBackend` trait and one backend per OS: a low-level hook on Windows, an event tap on macOS, evdev and uinput on Linux. All `unsafe` code lives here. |
| `crates/padless` | The command line interface. |

Behavior changes belong in `padless-core`, where they can be tested without a keyboard. A
backend should only translate native events to `KeyEvent` values and carry out the
`Response` the engine returns.

## Checks

Every commit must pass:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo deny check
```

Clippy runs with the pedantic group enabled for the whole workspace. To check another
platform's backend without that machine, add its target and run clippy against it, for
example:

```sh
rustup target add x86_64-unknown-linux-gnu aarch64-apple-darwin
cargo clippy --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
cargo clippy --target aarch64-apple-darwin --all-targets -- -D warnings
```

The minimum supported Rust version is set by `rust-version` in `Cargo.toml` and checked in
CI.

## Code style

- No comments or doc comments. Names and structure carry the meaning; if code needs a comment
  to be understood, restructure it.
- No `unwrap()` or `expect()` outside tests. No `todo!`, placeholders or dead code.
- `unsafe` only in `padless-platform`, in small functions behind safe interfaces.
- Libraries report errors with `thiserror`; `anyhow` is used only in the binary.
- Never log, print or store key contents. See [SECURITY.md](SECURITY.md).
- Add a dependency only when it removes real work.

## Tests

The state machine has unit tests and property tests in
`crates/padless-core/tests/engine_properties.rs`. A change to the engine should come with a
test that fails without it. When a property test finds a failure, keep the shrunk case as a
unit test.

Platform backends are hard to test automatically. When you change one, describe in the pull
request what you tried by hand and on which OS, desktop environment and keyboard layout.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) in the imperative mood with a
lowercase subject under 72 characters, for example `fix(platform): release grab on device
removal`. Add a body only when it explains something the diff does not. Keep commits small and
make sure each one builds and passes the checks.

Update `CHANGELOG.md` under `Unreleased` for user-visible changes.

## License

By contributing you agree that your work is dual licensed under the MIT and Apache-2.0
licenses, as described in the README.

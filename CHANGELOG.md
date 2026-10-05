# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Hold mode: hold a trigger key and type a code on the number row. Sequences shorter than
  `min_digits` replay the original keys so existing shortcuts keep working.
- Leader mode: a configurable chord starts capture; Enter or Space commits, Escape or a
  timeout cancels.
- `windows` code table with CP437 and Windows-1252 semantics, and a `unicode` table for
  decimal code points.
- User aliases that map codes to arbitrary strings.
- Configurable triggers: left or right Alt, Ctrl and Meta, and Caps Lock.
- Windows backend using a low-level keyboard hook and `SendInput`.
- macOS backend using a session event tap, with a clear error when Accessibility access is
  missing.
- Linux backend using an exclusive evdev grab and a uinput virtual device, typing text through
  XTest on X11, the virtual keyboard protocol on Wayland, or Ctrl+Shift+U input.
- `padless run`, `lookup`, `config path` and `config check` commands.

[Unreleased]: https://github.com/defied-labs/padless/commits/main

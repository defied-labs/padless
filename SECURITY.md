# Security policy

padless installs a global keyboard hook. It sees every key you press, in every application,
including passwords. This document states what it does with that access and how to report
problems.

## Guarantees

- **No logging of key contents.** No log statement at any level includes a key, a digit, a
  code or typed text. Logs contain lifecycle messages, configuration summaries, device names
  and errors.
- **No persistence.** Keystrokes are never written to disk. The only key data held in memory
  is the set of keys currently pressed and the digits of the code being entered, which are
  discarded when the code is typed, replayed or cancelled.
- **No network access.** padless contains no networking code and makes no connections. On
  Linux it talks to the local X server or Wayland compositor through their Unix sockets to
  type text, which is the only inter-process communication it performs.
- **Minimal privileges.** padless runs as your user. It never asks for or uses root.

These guarantees are part of the project's scope. Changes that weaken them will not be
accepted.

## What the required permissions imply

- **Linux**: membership in the `input` group lets any program you run read every keyboard
  and mouse, not just padless. The udev rule in the README gives the same group write access
  to `/dev/uinput`, which allows injecting input. Consider whether that trade-off is
  acceptable on multi-user machines.
- **macOS**: Accessibility access is granted per program. If you grant it to your terminal,
  every program started from that terminal inherits it. Granting it to the padless binary
  alone is narrower.
- **Windows**: low-level keyboard hooks need no special permission.

## Verifying a build

Release binaries are built by the GitHub Actions workflow in `.github/workflows/release.yml`
from the tagged commit. You can rebuild from source with `cargo build --release --locked`.
Dependencies are checked with `cargo deny` for known advisories and license policy.

## Reporting a vulnerability

Report vulnerabilities privately through GitHub's
[private vulnerability reporting](https://github.com/defied-labs/padless/security/advisories/new).
Do not open a public issue.

Please include the affected version and platform, the impact, and steps to reproduce. You
can expect an acknowledgement within a week. Fixes for confirmed issues are released as soon
as practical and credited in the changelog unless you prefer otherwise.

Only the latest release receives security fixes.

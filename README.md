# padless

[![CI](https://github.com/defied-labs/padless/actions/workflows/ci.yml/badge.svg)](https://github.com/defied-labs/padless/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

padless is a small background daemon that lets you type characters by decimal code on the
number row. It reproduces the Windows Alt+NumPad codes for laptops and compact keyboards that
have no numeric keypad, and runs on Windows, macOS and Linux.

Hold Alt, type `0233` on the number row, release Alt, and `é` is typed into the focused
application. Short sequences such as Alt+1 still reach the application as the shortcut they
were.

## How it works

padless installs a global keyboard hook and runs every key event through a small state
machine.

- **Hold mode** (default). While the trigger key is held, number-row digits are held back. When
  the trigger is released, the digits are resolved to a character and typed. If fewer than
  `min_digits` digits were entered, the original key presses are replayed unchanged, so Alt+1
  tab switching keeps working. Pressing any other key while digits are pending replays
  everything as typed and stops capturing until the trigger is released.
- **Leader mode**. A chord such as Ctrl+Shift+U starts capture. Digits are collected until
  Enter or Space types the character, Escape cancels, or the timeout expires.

The trigger itself is never delayed: Alt reaches applications immediately, so Alt+Tab and
Alt+drag behave as usual. Only the digits are held back. When a code is typed after Alt alone,
padless sends an unassigned key before releasing Alt so Windows applications do not open their
menu bar.

Digits are recognized by physical position, so the number row works on layouts such as AZERTY,
where it produces digits only with Shift.

## Code tables

| Table | Behavior |
| --- | --- |
| `windows` (default) | Matches Windows. Codes without a leading zero use code page 437 (`130` is `é`, `156` is `£`); codes with a leading zero use Windows-1252 (`0233` is `é`, `0128` is `€`). Values wrap modulo 256 as on Windows, so `321` is `A`. |
| `unicode` | The code is a decimal Unicode code point: `8364` is `€`, `128512` is U+1F600. Surrogates and values above U+10FFFF are rejected. |

Code `0` produces nothing in either table. Aliases defined in the configuration take
precedence over the table and can expand to any string.

## Install

Prebuilt binaries for Linux, macOS and Windows are attached to each
[release](https://github.com/defied-labs/padless/releases).

To build from source, use Rust 1.94 or newer:

```sh
cargo install --locked --git https://github.com/defied-labs/padless padless
```

On Linux the build needs no system libraries: X11 and Wayland are spoken through pure Rust
crates.

## Usage

```text
padless run              run in the foreground until Ctrl+C, SIGINT or SIGTERM
padless lookup <CODE>    print what a code resolves to with the current configuration
padless config path      print the configuration file location
padless config check     validate the configuration and print warnings
```

`--config <PATH>` selects a different file, and `-v` or `-vv` increases log detail. Logs go to
stderr.

```console
$ padless lookup 0151
—	U+2014	windows table
```

padless does not daemonize itself. Run it from your session's autostart, a systemd user
service, a launchd agent or the Windows Task Scheduler.

## Configuration

The file is optional; without it padless uses the defaults below. Default locations:

| Platform | Path |
| --- | --- |
| Linux | `~/.config/padless/config.toml` |
| macOS | `~/Library/Application Support/padless/config.toml` |
| Windows | `%APPDATA%\padless\config\config.toml` |

```toml
mode = "hold"         # "hold" or "leader"
table = "windows"     # "windows" or "unicode"
trigger = "left-alt"  # hold mode trigger
min_digits = 2        # fewer digits than this replay the original keys

[leader]
chord = "ctrl+shift+u"
timeout_ms = 3000

[aliases]
"0" = "°"
"8594" = "→"
"42" = "¯\\_(ツ)_/¯"
```

| Key | Default | Values |
| --- | --- | --- |
| `mode` | `"hold"` | `"hold"`, `"leader"` |
| `table` | `"windows"` | `"windows"`, `"unicode"` |
| `trigger` | `"left-alt"` | `left-alt`, `right-alt`, `left-ctrl`, `right-ctrl`, `left-meta`, `right-meta`, `caps-lock` |
| `min_digits` | `2` | 1 to 10 |
| `leader.chord` | `"ctrl+shift+u"` | zero or more of `ctrl`, `alt`, `shift`, `meta`, followed by a letter, a digit or `f1` to `f24`, joined with `+` |
| `leader.timeout_ms` | `3000` | 1 to 60000; the timer restarts with each digit |
| `aliases` | empty | code (1 to 10 digits, leading zeros significant) to non-empty string |

Modifiers in a chord match either side of the keyboard and must match exactly: `ctrl+u` does
not fire while Shift is also held. Letter names refer to positions on a US layout.

Unknown keys are rejected. `padless config check` also warns about choices that work but
conflict with common setups:

- `trigger = "right-alt"`: right Alt is AltGr on most non-US layouts and is needed for
  characters such as `@`, `{` or `€`.
- A leader chord with `ctrl+alt`: Windows reports AltGr as Ctrl+Alt.
- `min_digits = 1`: single-digit shortcuts such as Alt+1 are captured.

`caps-lock` as a trigger is swallowed when pressed. A tap without digits is replayed, so Caps
Lock still toggles.

## Platform setup

### Windows

No setup is needed. padless uses a low-level keyboard hook and `SendInput`.

### macOS

padless needs Accessibility access. Open System Settings > Privacy & Security >
Accessibility and enable the program that starts padless: your terminal while testing, the
padless binary when run by launchd. padless exits with an explanation when the permission is
missing.

### Linux

padless reads keyboards from `/dev/input` with an exclusive grab and re-emits keys through a
`uinput` virtual device. This works the same under X11, Wayland and the console, but it needs
access to both:

```sh
sudo usermod -aG input "$USER"
echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' \
  | sudo tee /etc/udev/rules.d/60-padless-uinput.rules
sudo modprobe uinput
```

Log out and back in, or reboot, for the group change and udev rule to apply. Membership in
`input` lets a program read every keyboard; see [SECURITY.md](SECURITY.md).

Typed text uses the first method that works:

1. X11 sessions: XTest, with the character bound temporarily to an unused keycode.
2. Wayland sessions: the `zwp_virtual_keyboard_v1` protocol, if the compositor offers it.
3. Otherwise: Ctrl+Shift+U followed by the hex code point and Space, which GTK applications
   and IBus understand.

The session is detected from `XDG_SESSION_TYPE`, `WAYLAND_DISPLAY` and `DISPLAY`, so a
service must inherit them. With systemd, run padless as a user service in a session that
exports them (`systemctl --user import-environment`):

```ini
[Unit]
Description=padless
PartOf=graphical-session.target

[Service]
ExecStart=%h/.cargo/bin/padless run
Restart=on-failure

[Install]
WantedBy=graphical-session.target
```

## Known limitations

- **Wayland**: sway, Hyprland, river and other wlroots-based compositors offer the virtual
  keyboard protocol. GNOME and KDE Plasma do not offer it to ordinary clients, so padless falls
  back to Ctrl+Shift+U, which only works in applications that support it. Qt and Electron
  applications without IBus generally do not.
- **Ctrl+Shift+U fallback**: hex digits are sent as key positions, which assumes a layout
  where the number row and `a` to `f` produce those characters, as US and QWERTZ layouts do.
- **Linux devices**: keyboards connected after padless starts are not captured until it is
  restarted. Devices that report pointer events as well as keys, such as some wireless combo
  receivers, are skipped rather than grabbed, so padless does not work on them.
- **Windows elevation**: Windows does not let a normal process inject input into elevated
  windows. padless still captures the digits there but cannot type the result. Run padless
  elevated if you need it in administrator windows. Secure desktops such as UAC prompts and
  the lock screen are not covered.
- **Windows synthetic input**: input injected by other programs, including remote desktop
  tools and macro software, is passed through untouched.
- **macOS**: Caps Lock cannot be the trigger, because macOS toggles it before event taps see
  it. Fields that enable Secure Input, such as password fields, disable event taps. F21 to F24
  do not exist.
- **Shortcuts with many digits**: in hold mode, a trigger shortcut with at least `min_digits`
  digits is consumed as a code. Raise `min_digits` or use leader mode if that conflicts with
  something you use.
- **Code 0 and control characters**: code `0` types nothing. Windows-1252 control positions,
  such as `09`, type the control character, as Windows does.

## Security and privacy

padless sees every key you press, so it is deliberately narrow:

- Key contents are never logged, at any log level.
- Keystrokes are never written to disk. In memory, padless keeps only which keys are currently
  held and the digits of the code being entered.
- padless makes no network connections and has no networking code.

Details are in [SECURITY.md](SECURITY.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.

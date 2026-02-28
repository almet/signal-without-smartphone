# Setup Signal without smartphone

A simple native GUI tool that helps you register a Signal account and link
Signal Desktop — no smartphone required.

## Features

- Native GUI on Linux, macOS and Windows (no browser, no HTML shipped)
- Register a new Signal account
- Captcha support for abuse prevention
- SMS verification
- Link Signal Desktop via device URI
- Small self-contained binary (no Node.js, no webview)
- signal-cli embedded in release binaries (no separate download)

## How it works

The tool provides a step-by-step wizard that wraps
[signal-cli](https://github.com/AsamK/signal-cli):

1. **Phone number** — enter your number with country code
2. **Captcha** (if required) — open the captcha page in your browser, solve it,
   copy the `signalcaptcha://` token and paste it in the tool
3. **Verify** — enter the 6-digit SMS code
4. **Link** — paste the `tsdevice://` URI from Signal Desktop's QR code

## Quick start

```bash
# 1. Build the app
cargo build --release

# 2. Run it
./target/release/signal-setup
```

Release binaries already include signal-cli. For local development builds without embedding,
you can still run `download-signal-cli.sh` or `download-signal-cli.ps1` to provide it.
To embed it in local builds, provide a `signal-cli-embed.tar.gz` (top-level `signal-cli/`)
or set `SIGNAL_CLI_ARCHIVE` to its path before building.

## Build requirements

- **Rust** (latest stable) — install from [rustup.rs](https://rustup.rs/)
- **Linux only** — a few system libraries for GPU/display:

  ```bash
  # Ubuntu / Debian
  sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
      libxkbcommon-dev libssl-dev

  # Fedora
  sudo dnf install libxcb-devel libxkbcommon-devel openssl-devel

  # Arch
  sudo pacman -S libxcb libxkbcommon openssl
  ```

No Node.js, no npm, no webview dependencies.

## License

```
Signal Without Smartphone
Copyright (C) 2026 Alexis Métaireau

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published
by the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
```

## Credits

- GUI built with [egui](https://github.com/emilk/egui) /
  [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
- Uses [signal-cli](https://github.com/AsamK/signal-cli) by AsamK

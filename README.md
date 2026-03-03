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
# 1. Download signal-cli
./download-signal-cli.sh        # Linux / macOS
# OR
.\download-signal-cli.ps1       # Windows PowerShell

# 2. Build the app
cargo build --release

# 3. Run it (from the project root, next to the signal-cli/ directory)
./target/release/signal-setup
```

See [QUICKSTART.md](QUICKSTART.md) for detailed per-platform instructions.

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

This project is open source. The downloaded signal-cli binary is GPL-3.0
licensed — see the [signal-cli license](https://github.com/AsamK/signal-cli/blob/master/LICENSE).

## Credits

- GUI built with [egui](https://github.com/emilk/egui) /
  [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
- Uses [signal-cli](https://github.com/AsamK/signal-cli) by AsamK

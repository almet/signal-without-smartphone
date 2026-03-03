# Setup Signal without smartphone

A native GUI tool that helps you register a Signal account and link
Signal Desktop without requiring a smartphone.

*Signal requires access to phone number, though, so this utility avoids the need of a *smart* phone, but will still require a phone able to receive SMS messages.

## How it works

The tool provides a step-by-step wizard that talks **directly to Signal's HTTP
API**. The steps are as follow:

1. **Phone number** — enter your number with country code
2. **Captcha** (if required) — open the captcha page in your browser, solve it,
   copy the `signalcaptcha://` token and paste it in the tool
3. **Verify** — enter the 6-digit SMS code
4. **Link** — paste the `tsdevice://` URI from Signal Desktop's QR code

Under the hood the tool:
- Creates a registration session via `POST /v1/verification/session`
- Generates fresh Curve25519 identity keys and Kyber-1024 post-quantum pre-keys
- Signs pre-keys with XEdDSA (Signal's own signing scheme)
- Registers the account via `POST /v1/registration`
- Encrypts a provisioning message and delivers it to Signal Desktop via
  `PUT /v1/provisioning/{uuid}`

## Quick start

```bash
# 1. Build the app
cargo build --release

# 2. Run it
./target/release/signal-setup
```

See [QUICKSTART.md](QUICKSTART.md) for detailed per-platform instructions.

## Build requirements

- **Rust** (latest stable) — install from [rustup.rs](https://rustup.rs/)
- **Linux only** — a few system libraries for GPU/display:

  ```bash
  # Ubuntu / Debian
  sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
      libxkbcommon-dev libssl-dev protobuf-compiler

  # Fedora
  sudo dnf install libxcb-devel libxkbcommon-devel openssl-devel protobuf-compiler

  # Arch
  sudo pacman -S libxcb libxkbcommon openssl protobuf
  ```

No Node.js, no npm, no webview, no Java — just Rust.

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
- Cryptography: [x25519-dalek](https://github.com/dalek-cryptography/x25519-dalek),
  [xeddsa](https://codeberg.org/SpotNuts/xeddsa),
  [pqcrypto-kyber](https://github.com/rustpq/pqcrypto)

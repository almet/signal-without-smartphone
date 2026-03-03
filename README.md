# Setup Signal without smartphone

A native GUI tool that helps you register a Signal account and link
Signal Desktop without requiring a smartphone.

*Signal requires access to phone number, though, so this utility avoids the
need of a *smart* phone, but will still require a phone able to receive SMS
messages.*

Grab [https://github.com/almet/signal-without-smartphone/releases](the latest release)!

## Want to build it yourself?

```bash
cargo build --release
./target/release/signal-setup
```

## Build requirements

On Linux only, a few system libraries are useful for GPU/display:

```bash
# Ubuntu / Debian
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
libxkbcommon-dev libssl-dev protobuf-compiler

# Fedora
sudo dnf install libxcb-devel libxkbcommon-devel openssl-devel protobuf-compiler

# Arch
sudo pacman -S libxcb libxkbcommon openssl protobuf
```

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

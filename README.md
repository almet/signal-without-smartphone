<p align="center">
  <img src="crates/signal-setup/assets/logo.png" alt="signal-setup logo" width="160" height="160">
</p>

# Setup Signal without smartphone

A Desktop application to register an account with Signal and link it with
Signal Desktop, all without requiring a smartphone.

![Capture of the interface](interface.png)

*Note that Signal still requires a phone number to be used. This utility avoids the
need of a *smart*phone, but will still require a phone able to receive SMS
messages during the setup phase*

Grab [the latest release!](https://github.com/almet/signal-without-smartphone/releases)

## But, why?

There are multiple reasons why this tool might be interesting to you:

### Don't compromise the security of your messages with a smartphone

The security of your signal conversations is as low as the security of the devices on which it is installed:

> If your device is lost or stolen when it’s not locked with your passcode, of course someone could read the messages on it. Likewise, law enforcement entities are known to use forensic tools to break into seized devices, making it possible to read anything on the device, including your messages.
> 
> — [Signal group safety, Freedom of the Press Foundation](https://freedom.press/digisec/blog/signal-group-safety/)

### You might just don't have a smartphone

Some people don't have a smartphone, and they should be able to use Signal :-)

## Why a separate tool?

Signal has no intention of supporting this use case in their own software.
Standalone registration code does exist in Signal Desktop, but it is gated to
development and staging builds only, and the maintainers have confirmed on
their tracker that it is not meant for end users
([#1118](https://github.com/signalapp/Signal-Desktop/issues/1118),
[#551](https://github.com/signalapp/Signal-Desktop/issues/551),
[#575](https://github.com/signalapp/Signal-Desktop/issues/575),
[#6431](https://github.com/signalapp/Signal-Desktop/issues/6431)).
That is why this separate tool exists.

## Install

Download the file for your system from the
[releases page](https://github.com/almet/signal-without-smartphone/releases):

- **macOS**: `signal-setup-macos-silicon.dmg` (Apple Silicon) or
  `signal-setup-macos-intel.dmg` (Intel). Open the `.dmg` and drag
  **Signal Setup** into Applications.
- **Linux**: `Signal_Setup-x86_64.AppImage` (or the arm64 build). Make it
  executable (`chmod +x Signal_Setup-*.AppImage`) and run it. A plain
  `signal-setup-linux-*` binary is also published if you prefer.
- **Windows**: `signal-setup-windows-amd64.exe`. Double-click to run.

### First launch on macOS and Windows

The releases are not yet signed with a paid developer certificate, so the
system warns you the first time:

- **macOS** shows "cannot be opened because it is from an unidentified
  developer." Right-click the app and choose **Open**, then confirm. You only
  need to do this once. If it reports the app is "damaged," clear the download
  quarantine flag: `xattr -dr com.apple.quarantine "/Applications/Signal Setup.app"`.

- **Windows** SmartScreen shows "Windows protected your PC." Click **More
  info**, then **Run anyway**.

## Want to build it yourself?

```bash
cargo build --release
./target/release/signal-setup
```

The project is a Cargo workspace with two crates: `signal-setup-core` (the
Signal registration and device-linking logic) and `signal-setup` (the desktop
GUI that depends on it). Building the workspace produces the `signal-setup`
binary above.

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

## Testing with Signal's staging server

You can run the tool against Signal's staging environment instead of production,
which is useful for development and testing without risking your real account:

```bash
./target/release/signal-setup --staging
```

To complete the device-linking step, you'll need a Signal Desktop instance also
connected to the staging server. Building Signal Desktop from source does this
by default:

```bash
git clone https://github.com/signalapp/Signal-Desktop.git
cd Signal-Desktop
pnpm install
pnpm start
```

A `--demo` flag is also available for fully offline testing with fake data (no
server needed).

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

# Setup Signal without smartphone

A Desktop application to register an account with Signal and link it with
Signal Desktop, all without without requiring a smartphone.

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

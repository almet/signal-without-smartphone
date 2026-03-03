# Quick Start Guide

## Linux

### 1. Install system dependencies

**Ubuntu / Debian:**
```bash
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
    libxkbcommon-dev libssl-dev build-essential curl
```

**Fedora:**
```bash
sudo dnf install libxcb-devel libxkbcommon-devel openssl-devel gcc
```

**Arch:**
```bash
sudo pacman -S libxcb libxkbcommon openssl base-devel
```

### 2. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 3. Download signal-cli and build

```bash
./download-signal-cli.sh
cargo build --release
```

### 4. Run

```bash
./target/release/signal-setup
```

---

## macOS

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 2. Download signal-cli and build

```bash
./download-signal-cli.sh
cargo build --release
```

### 3. Run

```bash
./target/release/signal-setup
```

---

## Windows

### 1. Install prerequisites

1. **Rust** — download from [rustup.rs](https://rustup.rs/)
2. **Visual Studio C++ Build Tools** — required by Rust  
   (select "Desktop development with C++" in the installer)

### 2. Download signal-cli and build

Open PowerShell in the project directory:

```powershell
.\download-signal-cli.ps1
cargo build --release
```

### 3. Run

```powershell
.\target\release\signal-setup.exe
```

---

## Using the app

The wizard walks you through four steps:

1. **Phone number** — enter your number with country code (e.g. `+1234567890`)
2. **Captcha** (if required):
   - Click "Open captcha page" — your browser opens automatically
   - Complete the captcha
   - Right-click "Open Signal" → "Copy link address"
   - Paste the `signalcaptcha://...` token into the field
3. **Verify** — enter the 6-digit code you receive by SMS
4. **Link Signal Desktop**:
   - Open Signal Desktop → "Link to an existing device"
   - A QR code appears; scan it with a QR-reader app to extract the
     `tsdevice://` URI
   - Paste the URI into the tool

---

## Troubleshooting

### "signal-cli not found"
Run the download script from the project root:
```bash
./download-signal-cli.sh   # Linux / macOS
.\download-signal-cli.ps1  # Windows
```
Then run the app from the same directory (it looks for `signal-cli/bin/` next to itself).

### Captcha page doesn't open automatically
Open it manually: <https://signalcaptchas.org/registration/generate.html>

### Build fails on Linux ("xcb" or "xkbcommon" errors)
Install the system libraries listed in step 1 above.

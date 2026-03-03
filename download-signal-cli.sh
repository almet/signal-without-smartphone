#!/bin/bash
set -e

SIGNAL_CLI_VERSION="v0.13.22"
BASE_URL="https://github.com/AsamK/signal-cli/releases/download/${SIGNAL_CLI_VERSION}"

# Create directory for signal-cli binaries
mkdir -p signal-cli

echo "Downloading signal-cli ${SIGNAL_CLI_VERSION}..."

# Detect platform - only Linux has native builds, others use Java version
case "$(uname -s)" in
    Linux*)
        PLATFORM="linux"
        ARCHIVE="signal-cli-${SIGNAL_CLI_VERSION:1}-Linux-native.tar.gz"
        NATIVE=true
        ;;
    Darwin*)
        PLATFORM="macos"
        ARCHIVE="signal-cli-${SIGNAL_CLI_VERSION:1}.tar.gz"
        NATIVE=false
        ;;
    MINGW*|MSYS*|CYGWIN*)
        PLATFORM="windows"
        ARCHIVE="signal-cli-${SIGNAL_CLI_VERSION:1}.tar.gz"
        NATIVE=false
        ;;
    *)
        echo "Unsupported platform: $(uname -s)"
        exit 1
        ;;
esac

echo "Detected platform: ${PLATFORM}"
echo "Native build available: ${NATIVE}"
echo "Downloading ${ARCHIVE}..."

# Download the archive
curl -fSL -o signal-cli-archive "${BASE_URL}/${ARCHIVE}"

echo "Extracting archive..."
tar -xzf signal-cli-archive

# The native version extracts as a single binary file
if [ "$NATIVE" = true ] && [ -f "signal-cli" ]; then
    echo "Native binary detected, creating directory structure..."
    rm -rf signal-cli-dir
    mkdir -p signal-cli-dir/bin
    mv signal-cli signal-cli-dir/bin/
    chmod +x signal-cli-dir/bin/signal-cli
    rm -rf signal-cli
    mv signal-cli-dir signal-cli
else
    # Find the extracted directory (for Java versions)
    EXTRACTED_DIR=$(find . -maxdepth 1 -type d -name "signal-cli-*" | head -1)

    if [ -z "$EXTRACTED_DIR" ]; then
        echo "Error: Could not find extracted directory or binary"
        exit 1
    fi

    echo "Moving files from ${EXTRACTED_DIR}..."
    rm -rf signal-cli
    mv "$EXTRACTED_DIR" signal-cli
fi

# Clean up
rm -f signal-cli-archive

echo "✓ signal-cli downloaded and extracted to signal-cli/"
echo "✓ Ready to build!"

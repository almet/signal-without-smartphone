# PowerShell script for Windows
$ErrorActionPreference = "Stop"

$SIGNAL_CLI_VERSION = "v0.13.22"
$VERSION_NUMBER = $SIGNAL_CLI_VERSION.Substring(1)
$BASE_URL = "https://github.com/AsamK/signal-cli/releases/download/$SIGNAL_CLI_VERSION"
# No Windows native build available, use Java version
$ARCHIVE = "signal-cli-$VERSION_NUMBER.tar.gz"

Write-Host "Downloading signal-cli $SIGNAL_CLI_VERSION (Java version)..."

# Create directory for signal-cli binaries
New-Item -ItemType Directory -Force -Path "signal-cli" | Out-Null

# Download the archive
$archivePath = "signal-cli-archive.tar.gz"
Write-Host "Downloading $ARCHIVE..."
Invoke-WebRequest -Uri "$BASE_URL/$ARCHIVE" -OutFile $archivePath

Write-Host "Extracting archive..."

# Extract the tar.gz using tar (available on Windows 10+)
tar -xzf "signal-cli-archive.tar.gz"

# Find the extracted directory
$extractedDir = Get-ChildItem -Path "." -Directory | Where-Object { $_.Name -like "signal-cli-*" } | Select-Object -First 1

if (-not $extractedDir) {
    Write-Error "Could not find extracted directory"
    exit 1
}

Write-Host "Moving files from $($extractedDir.Name)..."

# Remove old signal-cli directory if exists
if (Test-Path "signal-cli") {
    Remove-Item -Path "signal-cli" -Recurse -Force
}

# Move the extracted directory
Move-Item -Path $extractedDir.FullName -Destination "signal-cli"

# Clean up
Remove-Item -Path $archivePath

Write-Host "signal-cli downloaded and extracted to signal-cli/"
Write-Host "Ready to build!"

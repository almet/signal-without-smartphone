# GitHub Actions Workflows

## Release Workflow

The `release.yml` workflow automatically builds binaries for all supported platforms when you push a version tag.

### Supported Platforms

- **Linux AMD64** (x86_64-unknown-linux-gnu)
- **Linux ARM64** (aarch64-unknown-linux-gnu) - uses cross-compilation
- **macOS Intel** (x86_64-apple-darwin)
- **macOS Apple Silicon** (aarch64-apple-darwin)
- **Windows AMD64** (x86_64-pc-windows-msvc)

### How to Create a Release

1. Update the version in `Cargo.toml`
2. Commit your changes
3. Create and push a version tag:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```

4. GitHub Actions will automatically:
   - Build binaries for all platforms
   - Create a GitHub release
   - Upload all binaries as release assets
   - Generate release notes

### Binary Naming

Release binaries are named according to this pattern:
- `signal-setup-linux-amd64` - Linux x86_64
- `signal-setup-linux-arm64` - Linux ARM64
- `signal-setup-macos-intel` - macOS Intel
- `signal-setup-macos-silicon` - macOS Apple Silicon
- `signal-setup-windows-amd64.exe` - Windows x86_64

### Build Optimizations

The release builds use optimized settings from `Cargo.toml`:
- Size optimization (`opt-level = "z"`)
- Link-time optimization (LTO)
- Symbol stripping
- Panic abort (reduces binary size)

Expected binary sizes: ~5-6 MB per platform

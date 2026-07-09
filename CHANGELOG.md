# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]


## [3.3.0]- 2026-07-09

- Add the ability to refresh accounts to avoid being disconnected on linked accounts. The refreshing is done by pinging the server's "whoami" endpoint. See #12. 

## [3.2.0] - 2026-07-05

- Publish an empty profile right after registration. This is useful for
  Signal to issue profile key credentials, making it possible to create groups
  and accept invitations. Otherwise creating a group reports "user could not be
  found" and accepting an invitation silently does nothing). Fixes #11.
- Disable discoverability of the phone number by default, but make it possible to
  opt-in.

## [3.1.2] - 2026-05-29

- Ship a Linux AppImage to ease installation.
- 
## [3.1.1] - 2026-05-28


- Bundle all per-account secrets into a single OS-keyring entry per phone.
  Cuts macOS Keychain prompts from one per secret field to one per
  account.
- Cache the saved-account list in memory so the welcome screen no longer
  hits the OS keyring on every repaint.
- Include the matching `CHANGELOG.md` section in each GitHub release's
  notes, on top of the auto-generated commit summary.

## [3.1.0] - 2026-05-24

- Store account secrets (password, ACI/PNI identity key pairs, master key,
  profile key) in the OS-native keyring instead of `accounts.json`:
  macOS Keychain, Windows Credential Manager, Linux Secret Service.
- Per-account Signal Desktop profiles (`--user-data-dir`), so multiple
  registered numbers can coexist and run side-by-side on the same machine.
- "Launch Signal Desktop" buttons on the linking, completion, and welcome
  screens.
- "Re-link" action on the completion screen.
- One-shot migration of `accounts.json` from earlier builds: secrets are
  moved into the keyring and the file is rewritten without them.
- macOS releases ship as ad-hoc-signed `.app` bundles inside per-arch
  `.dmg` images (Intel + Apple Silicon) instead of raw binaries.

## [3.0.2] - 2026-05-11

- Pin Rust toolchain in `dtolnay/rust-toolchain` GitHub Action.

## [3.0.1] - 2026-05-11

- Auto-detect Signal production certificate changes in CI.
- Update bundled `signal-root.crt` to match Signal production servers.
- Pin GitHub Actions versions; bump runner versions.

## [3.0.0] - 2026-04-10

- Add demo and staging modes.
- Split `main.rs` into focused modules (app, ui theme/widgets/steps, qr).
- Split `signal_http.rs` into focused submodules.
- Add interface screenshot to the README.

## [2.0.1] - (folded into 3.0.x)

Same set of changes as 3.0.1, released from a parallel branch.

## [2.0.0] - 2026-03-03

- Replace `signal-cli` with a pure-Rust Signal HTTP client. No JVM
  required.
- Use `libsignal-protocol` for Signal Protocol encryption during the
  device-sync step.
- Build platform binaries on their native runners (macOS on macOS,
  etc.).
- Vendor build-time dependencies so the release build runs on a clean
  runner.

## [1.0.4] - 2026-02-28

- Fix binary location in the release archive.

## [1.0.3] - 2026-02-28

- Embed `signal-cli` in the release so users don't have to install Java
  separately.

## [1.0.2] - 2026-02-28

- macOS build moved to GitHub-hosted runners.
- Updated macOS runner image.

## [1.0.1] - 2026-02-28

- Add licensing info.
- Update release workflow.
- Switch hosting to Forgejo (Codeberg) with GitHub mirroring.

## [1.0.0] - 2026-02-28

- Replace the Tauri + Preact UI with a native egui GUI. No HTML or JS
  shipped.
- Light-themed UI with step indicator, card layout, and coloured status
  banners.
- Bump bundled `signal-cli` to v0.13.24.

## [0.1.0] - 2026-01-15

- Initial public release.
- Tauri + Preact frontend driving an embedded `signal-cli` to register a
  Signal account and link Signal Desktop without a smartphone.
- GitHub Actions workflow publishing binaries on release.

[Unreleased]: https://github.com/almet/signal-without-smartphone/compare/v3.3.0...HEAD

[3.3.0]: https://github.com/almet/signal-without-smartphone/compare/v3.3.0...v3.2.0
[3.2.0]: https://github.com/almet/signal-without-smartphone/compare/v3.1.2...v3.2.0
[3.1.2]: https://github.com/almet/signal-without-smartphone/compare/v3.1.1...v3.1.2
[3.1.1]: https://github.com/almet/signal-without-smartphone/compare/v3.1.0...v3.1.1
[3.1.0]: https://github.com/almet/signal-without-smartphone/compare/v3.0.2...v3.1.0
[3.0.2]: https://github.com/almet/signal-without-smartphone/compare/v3.0.1...v3.0.2
[3.0.1]: https://github.com/almet/signal-without-smartphone/compare/v3.0.0...v3.0.1
[3.0.0]: https://github.com/almet/signal-without-smartphone/compare/v2.0.0...v3.0.0
[2.0.0]: https://github.com/almet/signal-without-smartphone/compare/v1.0.4...v2.0.0
[1.0.4]: https://github.com/almet/signal-without-smartphone/compare/v1.0.3...v1.0.4
[1.0.3]: https://github.com/almet/signal-without-smartphone/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/almet/signal-without-smartphone/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/almet/signal-without-smartphone/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/almet/signal-without-smartphone/compare/v0.1.0...v1.0.0
[0.1.0]: https://github.com/almet/signal-without-smartphone/releases/tag/v0.1.0

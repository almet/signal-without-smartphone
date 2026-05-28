//! Core library for `signal-setup`: a pure-Rust Signal account registration
//! and device-linking implementation that talks directly to Signal's service
//! endpoints over HTTPS, with no Java runtime or signal-cli binary required.
//!
//! Registration flow:
//!   1. `request_verification_code` sends an SMS/voice code via Signal's API.
//!   2. `verify_and_register` verifies the code and registers the account,
//!      generating all required cryptographic keys.
//!
//! Device-linking flow (after registration):
//!   3. `link_device` parses a `tsdevice://` (or `sgnl://linkdevice`) URI from
//!      Signal Desktop's QR code and provisions Desktop as a linked device via
//!      Signal's provisioning API, then sends the initial sync messages using
//!      libsignal-protocol.
//!
//! The library is split into cohesive modules: `types` (account data types),
//! `proto` (inline protobuf wire types), `crypto` (pure cryptographic
//! helpers), `http` (the network flows and transport), `persistence`
//! (on-disk + keyring storage), `desktop` (Signal Desktop integration), and
//! `demo` (a fake backend for the UI demo mode).

pub mod http;
pub mod types;

mod crypto;
mod proto;

pub mod demo;
pub mod desktop;
pub mod persistence;

pub use http::*;
pub use types::*;

//! The per-step screens of the registration flow, one module per step.
//!
//! Each submodule adds one `ui_*` method to `SignalSetupApp` via its own
//! `impl` block. The update loop in `crate::app` calls them based on the
//! current `Step`.

mod captcha;
mod complete;
mod linking;
mod phone;
mod verify;
mod welcome;

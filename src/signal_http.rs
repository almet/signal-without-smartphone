//! Signal HTTP API client — a pure-Rust replacement for signal-cli.
//!
//! This module talks directly to Signal's service endpoints over HTTPS,
//! eliminating the need for a Java runtime or the signal-cli binary.
//!
//! Registration flow:
//!   1. `request_verification_code` → sends an SMS/voice code via Signal's API
//!   2. `verify_and_register`       → verifies the code and registers the account
//!                                    (generates all required cryptographic keys)
//!
//! Device-linking flow (after registration):
//!   3. `link_device` → parses a `tsdevice://` (or `sgnl://linkdevice`) URI from
//!                      Signal Desktop's QR code and provisions Desktop as a linked
//!                      device via Signal's provisioning API, then sends the
//!                      initial sync messages using libsignal-protocol.

use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use base64::prelude::*;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use libsignal_protocol::{
    self as sigprot, CiphertextMessage, DeviceId, IdentityKey, IdentityKeyPair,
    InMemSignalProtocolStore, KyberPreKeyId, PreKeyBundle, ProtocolAddress,
    SignedPreKeyId,
};
use prost::Message as ProstMessage;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand::{Rng, RngCore};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

// ── rand 0.9 ↔ rand_core 0.6 compatibility ───────────────────────────────────
//
// `libsignal-protocol` uses rand 0.9, while `x25519-dalek` and `xeddsa` still
// use rand_core 0.6 traits. This wrapper bridges the two so a single `StdRng`
// instance can be passed to both APIs.

struct Rng06Compat<'a>(&'a mut StdRng);

impl rand_core_06::RngCore for Rng06Compat<'_> {
    fn next_u32(&mut self) -> u32 { self.0.next_u32() }
    fn next_u64(&mut self) -> u64 { self.0.next_u64() }
    fn fill_bytes(&mut self, dest: &mut [u8]) { self.0.fill_bytes(dest) }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core_06::Error> {
        self.0.fill_bytes(dest);
        Ok(())
    }
}

impl rand_core_06::CryptoRng for Rng06Compat<'_> {}

// ── Signal service base URL ────────────────────────────────────────────────────

const SIGNAL_API: &str = "https://chat.signal.org";

// ── Inline protobuf types (avoids build.rs / protoc dependency) ───────────────

/// Signal's provisioning message, sent by a primary device to a secondary device.
/// Field numbers match `Provisioning.proto` from Signal's repository.
#[derive(Clone, PartialEq, ProstMessage)]
struct ProvisionMessage {
    #[prost(bytes = "vec", optional, tag = "1")]
    aci_identity_key_public: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    aci_identity_key_private: Option<Vec<u8>>,
    #[prost(string, optional, tag = "3")]
    number: Option<String>,
    #[prost(string, optional, tag = "4")]
    provisioning_code: Option<String>,
    #[prost(string, optional, tag = "5")]
    user_agent: Option<String>,
    #[prost(bytes = "vec", optional, tag = "6")]
    profile_key: Option<Vec<u8>>,
    #[prost(bool, optional, tag = "7")]
    read_receipts: Option<bool>,
    #[prost(string, optional, tag = "8")]
    aci: Option<String>,
    #[prost(uint32, optional, tag = "9")]
    provisioning_version: Option<u32>,
    #[prost(string, optional, tag = "10")]
    pni: Option<String>,
    #[prost(bytes = "vec", optional, tag = "11")]
    pni_identity_key_public: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "12")]
    pni_identity_key_private: Option<Vec<u8>>,
    /// Deprecated in newer Signal versions, but still required by Signal Desktop
    /// when linking: without it Desktop throws and refuses to complete provisioning.
    #[prost(bytes = "vec", optional, tag = "13")]
    master_key: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "17")]
    aci_binary: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "18")]
    pni_binary: Option<Vec<u8>>,
}

/// Envelope wrapping an encrypted `ProvisionMessage`.
#[derive(Clone, PartialEq, ProstMessage)]
struct ProvisionEnvelope {
    #[prost(bytes = "vec", optional, tag = "1")]
    public_key: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    body: Option<Vec<u8>>,
}

// ── Signal Protocol wire types (for proactive post-link sync) ──────────────────

/// Plaintext content wrapper before Signal Protocol encryption (SignalService.proto).
#[derive(Clone, PartialEq, ProstMessage)]
struct ContentProto {
    #[prost(message, optional, tag = "2")]
    sync_message: Option<SyncMsgProto>,
}

/// Minimal SyncMessage — contacts.isComplete = true + empty blocked list
/// tells Signal Desktop that sync is done and there are no existing contacts.
#[derive(Clone, PartialEq, ProstMessage)]
struct SyncMsgProto {
    #[prost(message, optional, tag = "1")]
    contacts: Option<SyncContactsProto>,
    #[prost(message, optional, tag = "4")]
    blocked: Option<SyncBlockedProto>,
}

#[derive(Clone, PartialEq, ProstMessage)]
struct SyncContactsProto {
    /// `true` means "I've sent all my contacts (there are none)."
    #[prost(bool, optional, tag = "6")]
    is_complete: Option<bool>,
}

/// Empty blocked list.
#[derive(Clone, PartialEq, ProstMessage)]
struct SyncBlockedProto {}


/// All cryptographic material for a registered Signal account.
///
/// Created by `verify_and_register` and required for `link_device`.
#[derive(Clone)]
pub struct SignalAccount {
    pub phone: String,
    /// Base64-encoded random password used for HTTP basic auth.
    pub password: String,
    /// ACI identity key pair (Account Identifier) — libsignal-protocol types.
    aci_identity: IdentityKeyPair,
    /// PNI identity key pair (Phone Number Identity) — libsignal-protocol types.
    pni_identity: IdentityKeyPair,
    /// ACI UUID returned by Signal after successful registration.
    pub aci: Option<String>,
    /// PNI uuid returned by Signal after successful registration.
    pub pni: Option<String>,
    /// 32-byte master key, generated once and included in every provisioning message.
    master_key: Vec<u8>,
    /// 32-byte random profile key.
    profile_key: Vec<u8>,
    /// 14-bit random registration ID, included in Signal Protocol message headers.
    registration_id: u32,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Error returned by all Signal API calls.
#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Signal API error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("Captcha required")]
    CaptchaRequired,
    /// The existing account on this number supports device-to-device data
    /// transfer. Signal requires the caller to explicitly opt out before
    /// allowing a fresh registration. Retry with `skip_device_transfer = true`.
    #[error("Device transfer available")]
    DeviceTransferAvailable,
    #[error("Invalid URI: {0}")]
    InvalidUri(String),
    #[error("Signal Protocol error: {0}")]
    Protocol(#[from] sigprot::SignalProtocolError),
    #[error("{0}")]
    Other(String),
}

/// Result of `request_verification_code`.
pub enum VerificationRequest {
    /// Code sent; caller must supply the session id to `verify_and_register`.
    CodeSent { session_id: String },
    /// Signal requires captcha before sending the code.
    CaptchaRequired { session_id: String },
}

// ── Step 1: create a verification session (request SMS code) ─────────────────

/// Ask Signal to start a registration session for `phone`.
///
/// Returns `VerificationRequest::CaptchaRequired` if Signal wants the user to
/// solve a captcha before it sends the SMS code.
pub fn request_verification_code(
    phone: &str,
    captcha: Option<&str>,
) -> Result<VerificationRequest, SignalError> {
    let client = build_client();

    // ── 1a. Create session ─────────────────────────────────────────────────
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct CreateSessionBody<'a> {
        number: &'a str,
        push_token: Option<()>,
        mcc: Option<()>,
        mnc: Option<()>,
        push_token_type: Option<()>,
    }

    let resp = client
        .post(format!("{SIGNAL_API}/v1/verification/session"))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .json(&CreateSessionBody {
            number: phone,
            push_token: None,
            mcc: None,
            mnc: None,
            push_token_type: None,
        })
        .send()?;

    let session: RegistrationSessionResponse = parse_response(resp)?;
    let session_id = session.id.clone();

    // ── 1b. Submit captcha if already provided ─────────────────────────────
    let session = if let Some(token) = captcha {
        patch_session_with_captcha(&client, &session_id, token)?
    } else {
        session
    };

    // ── 1c. Check if captcha is still needed ───────────────────────────────
    if session.captcha_required() {
        return Ok(VerificationRequest::CaptchaRequired { session_id });
    }

    // ── 1d. Request the SMS code ───────────────────────────────────────────
    if !session.allowed_to_request_code {
        return Err(SignalError::Other(
            "Server does not allow requesting a code at this time.".into(),
        ));
    }

    #[derive(Serialize)]
    struct RequestCodeBody {
        client: &'static str,
        transport: &'static str,
    }

    let resp = client
        .post(format!(
            "{SIGNAL_API}/v1/verification/session/{session_id}/code"
        ))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .json(&RequestCodeBody {
            client: "ows",
            transport: "sms",
        })
        .send()?;

    let _ = parse_response::<RegistrationSessionResponse>(resp)?;
    Ok(VerificationRequest::CodeSent { session_id })
}

/// Submit a captcha token and retrieve the updated session.
pub fn submit_captcha(
    session_id: &str,
    captcha_token: &str,
) -> Result<VerificationRequest, SignalError> {
    let client = build_client();
    let session = patch_session_with_captcha(&client, session_id, captcha_token)?;

    if session.captcha_required() {
        return Ok(VerificationRequest::CaptchaRequired {
            session_id: session.id,
        });
    }

    if !session.allowed_to_request_code {
        return Err(SignalError::Other(
            "Server does not allow requesting a code at this time.".into(),
        ));
    }

    #[derive(Serialize)]
    struct RequestCodeBody {
        client: &'static str,
        transport: &'static str,
    }

    let resp = client
        .post(format!(
            "{SIGNAL_API}/v1/verification/session/{session_id}/code"
        ))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .json(&RequestCodeBody {
            client: "ows",
            transport: "sms",
        })
        .send()?;

    let _ = parse_response::<RegistrationSessionResponse>(resp)?;
    Ok(VerificationRequest::CodeSent {
        session_id: session.id,
    })
}

// ── Step 2: verify code and register ─────────────────────────────────────────

/// Verify the user-supplied `code` and register the account with Signal.
///
/// Generates fresh identity keys, signed pre-keys, and Kyber last-resort keys;
/// submits them all to Signal's `/v1/registration` endpoint.
///
/// On success returns a `SignalAccount` that must be kept alive for the device-
/// linking step.
pub fn verify_and_register(
    phone: &str,
    session_id: &str,
    code: &str,
    skip_device_transfer: bool,
) -> Result<SignalAccount, SignalError> {
    let client = build_client();
    let mut rng = StdRng::from_os_rng();

    // ── 2a. Submit the verification code ──────────────────────────────────
    #[derive(Serialize)]
    struct SubmitCodeBody<'a> {
        code: &'a str,
    }
    #[derive(Deserialize)]
    struct SubmitCodeResponse {
        verified: bool,
    }

    let resp = client
        .put(format!(
            "{SIGNAL_API}/v1/verification/session/{session_id}/code"
        ))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .json(&SubmitCodeBody { code })
        .send()?;

    // 409 means the session is already verified (happens on retries after a
    // DeviceTransferAvailable response).
    let already_verified = resp.status().as_u16() == 409;
    let verified: SubmitCodeResponse = if already_verified {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        SubmitCodeResponse {
            verified: body.get("verified").and_then(|v| v.as_bool()).unwrap_or(false),
        }
    } else {
        parse_response(resp)?
    };
    if !verified.verified {
        return Err(SignalError::Other("Verification code was not accepted.".into()));
    }

    // ── 2b. Generate account credentials ─────────────────────────────────
    let password = random_password(&mut rng);

    // ── 2c. Generate identity key pairs using libsignal-protocol ──────────
    let aci_identity = IdentityKeyPair::generate(&mut rng);
    let pni_identity = IdentityKeyPair::generate(&mut rng);

    // ── 2d. Generate signed pre-keys (Curve25519) ─────────────────────────
    let aci_spk_pair = sigprot::KeyPair::generate(&mut rng);
    let aci_spk_sig = aci_identity
        .private_key()
        .calculate_signature(&aci_spk_pair.public_key.serialize(), &mut rng)
        .map_err(|e| SignalError::Other(format!("sign ACI SPK: {e}")))?;

    let pni_spk_pair = sigprot::KeyPair::generate(&mut rng);
    let pni_spk_sig = pni_identity
        .private_key()
        .calculate_signature(&pni_spk_pair.public_key.serialize(), &mut rng)
        .map_err(|e| SignalError::Other(format!("sign PNI SPK: {e}")))?;

    // ── 2e. Generate Kyber-1024 last-resort pre-keys ──────────────────────
    let aci_kyber = sigprot::kem::KeyPair::generate(sigprot::kem::KeyType::Kyber1024, &mut rng);
    let aci_kyber_sig = aci_identity
        .private_key()
        .calculate_signature(&aci_kyber.public_key.serialize(), &mut rng)
        .map_err(|e| SignalError::Other(format!("sign ACI Kyber: {e}")))?;

    let pni_kyber = sigprot::kem::KeyPair::generate(sigprot::kem::KeyType::Kyber1024, &mut rng);
    let pni_kyber_sig = pni_identity
        .private_key()
        .calculate_signature(&pni_kyber.public_key.serialize(), &mut rng)
        .map_err(|e| SignalError::Other(format!("sign PNI Kyber: {e}")))?;

    // ── 2f. Other account attributes ─────────────────────────────────────
    let registration_id: u32 = rng.random_range(1..=16383);
    let pni_registration_id: u32 = rng.random_range(1..=16383);
    let mut unidentified_access_key = [0u8; 16];
    rng.fill_bytes(&mut unidentified_access_key);

    let mut master_key = vec![0u8; 32];
    rng.fill_bytes(&mut master_key);

    let mut profile_key = vec![0u8; 32];
    rng.fill_bytes(&mut profile_key);

    // ── 2g. Build and send the registration request ───────────────────────
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct AccountAttributes {
        registration_id: u32,
        pni_registration_id: u32,
        fetches_messages: bool,
        capabilities: Capabilities,
        unidentified_access_key: String,
        unrestricted_unidentified_access: bool,
        discoverable_by_phone_number: bool,
        name: String,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Capabilities {
        storage: bool,
        transfer: bool,
        delete_sync: bool,
        versioned_expiration_timer: bool,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct SignedPreKeyJson {
        key_id: u32,
        public_key: String,
        signature: String,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct KyberPreKeyJson {
        key_id: u32,
        public_key: String,
        signature: String,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RegistrationBody {
        session_id: String,
        account_attributes: AccountAttributes,
        skip_device_transfer: bool,
        every_signed_key_valid: bool,
        aci_identity_key: String,
        pni_identity_key: String,
        aci_signed_pre_key: SignedPreKeyJson,
        pni_signed_pre_key: SignedPreKeyJson,
        aci_pq_last_resort_pre_key: KyberPreKeyJson,
        pni_pq_last_resort_pre_key: KyberPreKeyJson,
    }

    let body = RegistrationBody {
        session_id: session_id.to_string(),
        account_attributes: AccountAttributes {
            registration_id,
            pni_registration_id,
            fetches_messages: true,
            capabilities: Capabilities {
                storage: true,
                transfer: true,
                delete_sync: true,
                versioned_expiration_timer: true,
            },
            unidentified_access_key: BASE64_STANDARD.encode(unidentified_access_key),
            unrestricted_unidentified_access: false,
            discoverable_by_phone_number: true,
            name: String::new(),
        },
        skip_device_transfer,
        every_signed_key_valid: true,
        aci_identity_key: BASE64_STANDARD
            .encode(aci_identity.identity_key().serialize()),
        pni_identity_key: BASE64_STANDARD
            .encode(pni_identity.identity_key().serialize()),
        aci_signed_pre_key: SignedPreKeyJson {
            key_id: 1,
            public_key: BASE64_STANDARD.encode(aci_spk_pair.public_key.serialize()),
            signature: BASE64_STANDARD.encode(&aci_spk_sig),
        },
        pni_signed_pre_key: SignedPreKeyJson {
            key_id: 1,
            public_key: BASE64_STANDARD.encode(pni_spk_pair.public_key.serialize()),
            signature: BASE64_STANDARD.encode(&pni_spk_sig),
        },
        aci_pq_last_resort_pre_key: KyberPreKeyJson {
            key_id: 1,
            public_key: BASE64_STANDARD.encode(aci_kyber.public_key.serialize()),
            signature: BASE64_STANDARD.encode(&aci_kyber_sig),
        },
        pni_pq_last_resort_pre_key: KyberPreKeyJson {
            key_id: 1,
            public_key: BASE64_STANDARD.encode(pni_kyber.public_key.serialize()),
            signature: BASE64_STANDARD.encode(&pni_kyber_sig),
        },
    };

    let resp = client
        .post(format!("{SIGNAL_API}/v1/registration"))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .basic_auth(phone, Some(&password))
        .json(&body)
        .send()?;

    // 409 means an existing account on this number has the Transfer capability;
    // Signal requires the client to explicitly set skipDeviceTransfer=true.
    if resp.status().as_u16() == 409 {
        return Err(SignalError::DeviceTransferAvailable);
    }

    #[derive(Deserialize)]
    struct RegistrationResponse {
        #[serde(rename = "uuid")]
        aci: Option<String>,
        pni: Option<String>,
    }

    let reg: RegistrationResponse = parse_response(resp)?;

    Ok(SignalAccount {
        phone: phone.to_string(),
        password,
        aci_identity,
        pni_identity,
        aci: reg.aci,
        pni: reg.pni,
        master_key,
        profile_key,
        registration_id,
    })
}

// ── Step 3: link Signal Desktop ───────────────────────────────────────────────

/// Link Signal Desktop as a secondary device using a `tsdevice://` or
/// `sgnl://linkdevice` URI decoded from its QR code.
///
/// The `account` must come from a successful `verify_and_register` call.
pub fn link_device(account: &SignalAccount, device_uri: &str) -> Result<(), SignalError> {
    let client = build_client();
    let mut rng = StdRng::from_os_rng();

    // ── 3a. Parse the tsdevice:// URI ─────────────────────────────────────
    let (ephemeral_id, device_pub_key_bytes) = parse_device_uri(device_uri)?;

    // The key in the URI is DJB-encoded (0x05 prefix + 32 bytes).
    let key_slice = if device_pub_key_bytes.len() == 33 && device_pub_key_bytes[0] == 0x05 {
        &device_pub_key_bytes[1..]
    } else if device_pub_key_bytes.len() == 32 {
        &device_pub_key_bytes[..]
    } else {
        return Err(SignalError::InvalidUri(format!(
            "Unexpected public key length: {}",
            device_pub_key_bytes.len()
        )));
    };
    let key_arr: [u8; 32] = key_slice
        .try_into()
        .map_err(|_| SignalError::InvalidUri("Public key must be 32 bytes".into()))?;
    let device_pub = X25519Public::from(key_arr);

    // Signal's AccountAuthenticator requires the ACI UUID as the username.
    let aci = account.aci.as_deref().ok_or_else(|| {
        SignalError::Other("ACI UUID is missing; cannot authenticate with Signal".into())
    })?;

    // ── 3b. Obtain a provisioning code from Signal ────────────────────────
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DeviceCode {
        verification_code: String,
    }

    let resp = client
        .get(format!("{SIGNAL_API}/v1/devices/provisioning/code"))
        .header("X-Signal-Agent", "OWD")
        .basic_auth(aci, Some(&account.password))
        .send()?;
    let code: DeviceCode = parse_response(resp)?;

    // ── 3c. Build the ProvisionMessage with all required fields ────────────
    let pni_plain = account
        .pni
        .as_deref()
        .map(|p| p.strip_prefix("PNI:").unwrap_or(p).to_string());

    // Parse UUIDs into 16-byte binary form for aciBinary/pniBinary fields
    let aci_binary = parse_uuid_bytes(aci);
    let pni_binary = pni_plain.as_deref().and_then(parse_uuid_bytes);

    let msg = ProvisionMessage {
        aci_identity_key_public: Some(account.aci_identity.identity_key().serialize().to_vec()),
        aci_identity_key_private: Some(account.aci_identity.private_key().serialize()),
        pni_identity_key_public: Some(account.pni_identity.identity_key().serialize().to_vec()),
        pni_identity_key_private: Some(account.pni_identity.private_key().serialize()),
        number: Some(account.phone.clone()),
        provisioning_code: Some(code.verification_code),
        provisioning_version: Some(1), // ProvisioningVersion::TABLET_SUPPORT
        aci: Some(aci.to_string()),
        pni: pni_plain,
        profile_key: Some(account.profile_key.clone()),
        master_key: Some(account.master_key.clone()),
        user_agent: None,
        read_receipts: None,
        aci_binary,
        pni_binary,
    };

    let envelope = encrypt_provision_message(&msg, &device_pub, &mut rng)?;

    // ── 3d. Send the encrypted envelope to Signal's provisioning endpoint ─
    #[derive(Serialize)]
    struct SendEnvelope {
        body: String,
    }

    let envelope_bytes = envelope.encode_to_vec();
    let resp = client
        .put(format!(
            "{SIGNAL_API}/v1/provisioning/{ephemeral_id}"
        ))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .basic_auth(aci, Some(&account.password))
        .json(&SendEnvelope {
            body: BASE64_STANDARD.encode(&envelope_bytes),
        })
        .send()?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(SignalError::Api {
            status: status.as_u16(),
            body,
        });
    }

    // ── 3e. Proactively send empty sync to the new device ─────────────────
    // Signal Desktop waits for sync messages from device 1 after linking.
    // We send a SyncMessage{contacts.isComplete=true, blocked={}} so Desktop
    // knows sync is complete and transitions to the main screen.
    // We ignore errors here — Desktop will eventually time out gracefully.
    let _ = send_linked_device_sync(&client, account, &mut rng);

    Ok(())
}

// ── Step 3 helpers: proactive post-link sync using libsignal-protocol ─────────

/// Parsed pre-key bundle for establishing a Signal Protocol session.
struct DevicePreKeyBundle {
    identity_key: IdentityKey,
    registration_id: u32,
    signed_prekey_id: u32,
    signed_prekey_bytes: Vec<u8>,
    signed_prekey_signature: Vec<u8>,
    kyber_prekey_id: Option<u32>,
    kyber_prekey_bytes: Option<Vec<u8>>,
    kyber_prekey_signature: Option<Vec<u8>>,
    one_time_prekey_id: Option<u32>,
    one_time_prekey_bytes: Option<Vec<u8>>,
}

/// Fetch device 2's pre-key bundle, retrying for up to `timeout_secs` seconds
/// to give Signal Desktop time to register and upload its pre-keys.
fn fetch_device_prekeys(
    client: &Client,
    account: &SignalAccount,
    aci: &str,
    device_id: u32,
    timeout_secs: u64,
) -> Result<DevicePreKeyBundle, SignalError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct KeyResponse {
        identity_key: String,
        devices: Vec<DeviceKeys>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DeviceKeys {
        registration_id: u32,
        pre_key: Option<PreKeyEntry>,
        signed_pre_key: PreKeyEntry,
        #[serde(default)]
        pq_pre_key: Option<PqPreKeyEntry>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PreKeyEntry {
        key_id: u32,
        public_key: String,
        #[serde(default)]
        signature: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PqPreKeyEntry {
        key_id: u32,
        public_key: String,
        signature: String,
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let resp = client
            .get(format!("{SIGNAL_API}/v2/keys/{aci}/{device_id}"))
            .header("X-Signal-Agent", "OWD")
            .basic_auth(aci, Some(&account.password))
            .send()?;

        if resp.status().is_success() {
            let kr: KeyResponse = resp.json()?;
            if let Some(dev) = kr.devices.into_iter().next() {
                let identity_key_bytes =
                    BASE64_STANDARD.decode(&kr.identity_key).map_err(|e| {
                        SignalError::Other(format!("bad identity key: {e}"))
                    })?;
                let identity_key = IdentityKey::decode(&identity_key_bytes)
                    .map_err(|e| SignalError::Other(format!("decode identity key: {e}")))?;
                let signed_prekey_bytes =
                    BASE64_STANDARD
                        .decode(&dev.signed_pre_key.public_key)
                        .map_err(|e| SignalError::Other(format!("bad spk: {e}")))?;
                let signed_prekey_signature = dev
                    .signed_pre_key
                    .signature
                    .as_deref()
                    .map(|s| BASE64_STANDARD.decode(s))
                    .transpose()
                    .map_err(|e| SignalError::Other(format!("bad spk sig: {e}")))?
                    .unwrap_or_default();
                let (one_time_prekey_id, one_time_prekey_bytes) = match dev.pre_key {
                    Some(pk) => {
                        let raw = BASE64_STANDARD.decode(&pk.public_key).map_err(|e| {
                            SignalError::Other(format!("bad opk: {e}"))
                        })?;
                        (Some(pk.key_id), Some(raw))
                    }
                    None => (None, None),
                };
                let (kyber_id, kyber_bytes, kyber_sig) = match dev.pq_pre_key {
                    Some(pq) => {
                        let key_bytes = BASE64_STANDARD.decode(&pq.public_key).map_err(|e| {
                            SignalError::Other(format!("bad pq key: {e}"))
                        })?;
                        let sig_bytes = BASE64_STANDARD.decode(&pq.signature).map_err(|e| {
                            SignalError::Other(format!("bad pq sig: {e}"))
                        })?;
                        (Some(pq.key_id), Some(key_bytes), Some(sig_bytes))
                    }
                    None => (None, None, None),
                };
                return Ok(DevicePreKeyBundle {
                    identity_key,
                    registration_id: dev.registration_id,
                    signed_prekey_id: dev.signed_pre_key.key_id,
                    signed_prekey_bytes,
                    signed_prekey_signature,
                    kyber_prekey_id: kyber_id,
                    kyber_prekey_bytes: kyber_bytes,
                    kyber_prekey_signature: kyber_sig,
                    one_time_prekey_id,
                    one_time_prekey_bytes,
                });
            }
        }

        if std::time::Instant::now() >= deadline {
            return Err(SignalError::Other(
                "Timed out waiting for linked device pre-keys".into(),
            ));
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

/// After delivering the ProvisionMessage, wait for Signal Desktop to register as
/// device 2, then send it an empty `SyncMessage` (contacts complete, no blocked)
/// via Signal Protocol so it transitions out of the "Syncing…" waiting screen.
///
/// Uses `libsignal-protocol` for proper X3DH session establishment and
/// Double Ratchet encryption.
fn send_linked_device_sync(
    client: &Client,
    account: &SignalAccount,
    rng: &mut StdRng,
) -> Result<(), SignalError> {
    let aci = account
        .aci
        .as_deref()
        .ok_or_else(|| SignalError::Other("no ACI".into()))?;

    // Wait up to 60 s for Desktop to register and upload its pre-keys.
    let bundle = fetch_device_prekeys(client, account, aci, 2, 60)?;

    // Build the sync payload: empty contacts (complete) + empty blocked list.
    let plaintext = ContentProto {
        sync_message: Some(SyncMsgProto {
            contacts: Some(SyncContactsProto {
                is_complete: Some(true),
            }),
            blocked: Some(SyncBlockedProto {}),
        }),
    }
    .encode_to_vec();

    // Use libsignal-protocol for proper Signal Protocol encryption.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| SignalError::Other(format!("tokio runtime: {e}")))?;

    let wire = rt.block_on(async {
        encrypt_with_libsignal(&plaintext, account, &bundle, rng).await
    })?;

    // Send via Signal's message endpoint (device 1 → device 2, same account).
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct OutMsg {
        r#type: u32,
        destination_device_id: u32,
        destination_registration_id: u32,
        content: String,
    }
    #[derive(Serialize)]
    struct SendBody {
        messages: Vec<OutMsg>,
        timestamp: u64,
        online: bool,
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let msg_type: u32 = match wire.message_type() {
        sigprot::CiphertextMessageType::PreKey => 3,
        sigprot::CiphertextMessageType::Whisper => 1,
        other => {
            return Err(SignalError::Other(format!(
                "Unexpected ciphertext message type: {other:?}"
            )));
        }
    };

    let body = SendBody {
        messages: vec![OutMsg {
            r#type: msg_type,
            destination_device_id: 2,
            destination_registration_id: bundle.registration_id,
            content: BASE64_STANDARD.encode(wire.serialize()),
        }],
        timestamp,
        online: false,
    };

    let resp = client
        .put(format!("{SIGNAL_API}/v1/messages/{aci}"))
        .header("X-Signal-Agent", "OWD")
        .basic_auth(aci, Some(&account.password))
        .json(&body)
        .send()?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().unwrap_or_default();
        return Err(SignalError::Api { status, body });
    }

    Ok(())
}

/// Encrypt plaintext using libsignal-protocol's proper X3DH session
/// establishment and Double Ratchet encryption.
async fn encrypt_with_libsignal(
    plaintext: &[u8],
    account: &SignalAccount,
    bundle: &DevicePreKeyBundle,
    rng: &mut StdRng,
) -> Result<CiphertextMessage, SignalError> {
    let receiver_address = ProtocolAddress::new(
        account.aci.clone().unwrap_or_default(),
        DeviceId::try_from(2u32).expect("valid device id"),
    );

    // Create an in-memory protocol store for the sender (primary device)
    let mut store = InMemSignalProtocolStore::new(
        account.aci_identity.clone(),
        account.registration_id,
    )?;

    // Parse the signed pre-key public key from the bundle
    let signed_prekey_pub = sigprot::PublicKey::deserialize(&bundle.signed_prekey_bytes)
        .map_err(|e| SignalError::Other(format!("parse signed prekey: {e}")))?;

    // Build the pre-key bundle for session establishment.
    // We need the Kyber pre-key if available; if not, we need a fallback.
    let pre_key_bundle = if let (Some(kyber_id), Some(kyber_bytes), Some(kyber_sig)) = (
        bundle.kyber_prekey_id,
        bundle.kyber_prekey_bytes.as_ref(),
        bundle.kyber_prekey_signature.as_ref(),
    ) {
        let kyber_pub = sigprot::kem::PublicKey::deserialize(kyber_bytes)
            .map_err(|e| SignalError::Other(format!("parse kyber prekey: {e}")))?;

        // Build one-time prekey option
        let pre_key_opt = if let (Some(pk_id), Some(pk_bytes)) = (
            bundle.one_time_prekey_id,
            bundle.one_time_prekey_bytes.as_ref(),
        ) {
            let pk_pub = sigprot::PublicKey::deserialize(pk_bytes)
                .map_err(|e| SignalError::Other(format!("parse prekey: {e}")))?;
            Some((sigprot::PreKeyId::from(pk_id), pk_pub))
        } else {
            None
        };

        PreKeyBundle::new(
            bundle.registration_id,
            DeviceId::try_from(2u32).expect("valid device id"),
            pre_key_opt,
            SignedPreKeyId::from(bundle.signed_prekey_id),
            signed_prekey_pub,
            bundle.signed_prekey_signature.clone(),
            KyberPreKeyId::from(kyber_id),
            kyber_pub,
            kyber_sig.clone(),
            bundle.identity_key,
        )?
    } else {
        return Err(SignalError::Other(
            "Linked device did not provide Kyber pre-key; cannot establish session".into(),
        ));
    };

    // Process the pre-key bundle to establish a Signal Protocol session
    sigprot::process_prekey_bundle(
        &receiver_address,
        &mut store.session_store,
        &mut store.identity_store,
        &pre_key_bundle,
        std::time::SystemTime::now(),
        rng,
    )
    .await?;

    // Encrypt the message using the established session
    let ciphertext = sigprot::message_encrypt(
        plaintext,
        &receiver_address,
        &mut store.session_store,
        &mut store.identity_store,
        std::time::SystemTime::now(),
        rng,
    )
    .await?;

    Ok(ciphertext)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Build a `reqwest` blocking client that pins Signal's server certificate.
fn build_client() -> Client {
    use rustls::client::danger::{
        HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
    };
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
    use std::sync::Arc;

    let pem = include_str!("../signal-root.crt");
    let der_b64: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let pinned_der = BASE64_STANDARD
        .decode(der_b64.trim())
        .expect("Invalid base64 in signal-root.crt");

    #[derive(Debug)]
    struct PinnedCertVerifier {
        pinned_der: Vec<u8>,
    }

    impl ServerCertVerifier for PinnedCertVerifier {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, TlsError> {
            if end_entity.as_ref() == self.pinned_der.as_slice() {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(TlsError::General(
                    "Server certificate does not match pinned Signal certificate".into(),
                ))
            }
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, TlsError> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, TlsError> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![
                SignatureScheme::ED25519,
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::ECDSA_NISTP384_SHA384,
                SignatureScheme::RSA_PKCS1_SHA256,
                SignatureScheme::RSA_PKCS1_SHA384,
                SignatureScheme::RSA_PKCS1_SHA512,
                SignatureScheme::RSA_PSS_SHA256,
                SignatureScheme::RSA_PSS_SHA384,
                SignatureScheme::RSA_PSS_SHA512,
            ]
        }
    }

    let tls_config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("Failed to set TLS protocol versions")
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(PinnedCertVerifier { pinned_der }))
    .with_no_client_auth();

    Client::builder()
        .use_preconfigured_tls(tls_config)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client")
}

/// Send a PATCH to update the verification session (e.g. submit a captcha).
fn patch_session_with_captcha(
    client: &Client,
    session_id: &str,
    captcha_token: &str,
) -> Result<RegistrationSessionResponse, SignalError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct PatchBody<'a> {
        captcha: Option<&'a str>,
        push_token: Option<()>,
        push_challenge: Option<()>,
        mcc: Option<()>,
        mnc: Option<()>,
        push_token_type: Option<()>,
    }

    // Strip the "signalcaptcha://" scheme prefix that Signal Desktop/Android
    // includes in the URI but the server API does not expect.
    let captcha_token = captcha_token
        .strip_prefix("signalcaptcha://")
        .unwrap_or(captcha_token);

    let resp = client
        .patch(format!(
            "{SIGNAL_API}/v1/verification/session/{session_id}"
        ))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .json(&PatchBody {
            captcha: Some(captcha_token),
            push_token: None,
            push_challenge: None,
            mcc: None,
            mnc: None,
            push_token_type: None,
        })
        .send()?;

    parse_response(resp)
}

/// Deserialise a response body or return a `SignalError::Api`.
fn parse_response<T: for<'de> Deserialize<'de>>(
    resp: reqwest::blocking::Response,
) -> Result<T, SignalError> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        if status.as_u16() == 402 {
            return Err(SignalError::CaptchaRequired);
        }
        return Err(SignalError::Api {
            status: status.as_u16(),
            body,
        });
    }
    resp.json::<T>().map_err(Into::into)
}

/// Signal session response from the verification session API.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistrationSessionResponse {
    id: String,
    allowed_to_request_code: bool,
    #[serde(default)]
    requested_information: Vec<String>,
}

impl RegistrationSessionResponse {
    fn captcha_required(&self) -> bool {
        self.requested_information
            .iter()
            .any(|x| x.as_str() == "captcha")
    }
}

/// Generate a random password: 20 random bytes encoded as base64.
fn random_password(rng: &mut StdRng) -> String {
    let mut bytes = [0u8; 20];
    rng.fill_bytes(&mut bytes);
    BASE64_STANDARD.encode(bytes)
}

/// Prepend Signal's DJB Curve25519 key type byte (0x05) to a 32-byte key.
fn djb_key(key: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(33);
    v.push(0x05);
    v.extend_from_slice(key);
    v
}

/// Compute an XEdDSA signature of `message` using an X25519 `private_key`.
#[cfg(test)]
fn xeddsa_sign(private_key: &StaticSecret, message: &[u8], rng: &mut StdRng) -> [u8; 64] {
    use xeddsa::{xed25519::PrivateKey as XEdKey, Sign as XEdSign};
    let xed: XEdKey = private_key.into();
    xed.sign(message, Rng06Compat(rng))
}

/// Parse a UUID string into its 16-byte binary representation.
fn parse_uuid_bytes(uuid_str: &str) -> Option<Vec<u8>> {
    let hex: String = uuid_str.replace('-', "");
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = Vec::with_capacity(16);
    for i in (0..32).step_by(2) {
        bytes.push(u8::from_str_radix(&hex[i..i + 2], 16).ok()?);
    }
    Some(bytes)
}

/// Parse `tsdevice://` and `sgnl://linkdevice` URIs.
///
/// Returns `(ephemeral_id, device_public_key_bytes)`.
fn parse_device_uri(uri: &str) -> Result<(String, Vec<u8>), SignalError> {
    let query_str = if let Some(q) = uri.find('?') {
        &uri[q + 1..]
    } else {
        return Err(SignalError::InvalidUri(
            "URI has no query parameters".into(),
        ));
    };

    let params: HashMap<String, String> = query_str
        .split('&')
        .filter_map(|kv| {
            let mut parts = kv.splitn(2, '=');
            let k = parts.next()?;
            let v = parts.next()?;
            let v_decoded = percent_decode(v);
            Some((k.to_string(), v_decoded))
        })
        .collect();

    let uuid = params
        .get("uuid")
        .cloned()
        .ok_or_else(|| SignalError::InvalidUri("Missing 'uuid' parameter".into()))?;

    let pub_key_str = params
        .get("pub_key")
        .cloned()
        .ok_or_else(|| SignalError::InvalidUri("Missing 'pub_key' parameter".into()))?;

    let pub_key_bytes = BASE64_STANDARD_NO_PAD
        .decode(&pub_key_str)
        .or_else(|_| BASE64_URL_SAFE_NO_PAD.decode(&pub_key_str))
        .or_else(|_| BASE64_STANDARD.decode(&pub_key_str))
        .or_else(|_| BASE64_URL_SAFE.decode(&pub_key_str))
        .map_err(|e| SignalError::InvalidUri(format!("Bad base64 in pub_key: {e}")))?;

    Ok((uuid, pub_key_bytes))
}

/// Minimal percent-decoder for URI query values.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut decoded_bytes: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h = &bytes[i + 1..i + 3];
            if let Ok(hex) = std::str::from_utf8(h) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    decoded_bytes.push(byte);
                    i += 3;
                    continue;
                }
            }
        } else if bytes[i] == b'+' {
            decoded_bytes.push(b' ');
            i += 1;
            continue;
        }
        decoded_bytes.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&decoded_bytes).into_owned()
}

/// Encrypt a `ProvisionMessage` for delivery to `device_pub`.
///
/// Algorithm (matches Signal's `ProvisioningCipher`):
///   1. Generate ephemeral X25519 key pair.
///   2. ECDH with `device_pub` → 32-byte shared secret.
///   3. HKDF-SHA256 (no salt, info = "TextSecure Provisioning Message") → 64 bytes.
///      First 32 bytes = AES-256 key, last 32 bytes = HMAC-SHA256 key.
///   4. Encrypt the serialised proto with AES-256-CBC + PKCS7.
///   5. Authenticate with HMAC-SHA256 over `[VERSION || IV || CIPHERTEXT]`.
///   6. Return `ProvisionEnvelope { public_key: ephemeral_djb, body: VERSION || IV || CT || MAC }`.
fn encrypt_provision_message(
    msg: &ProvisionMessage,
    device_pub: &X25519Public,
    rng: &mut StdRng,
) -> Result<ProvisionEnvelope, SignalError> {
    const VERSION: u8 = 1;

    let ephemeral_secret = StaticSecret::random_from_rng(Rng06Compat(rng));
    let ephemeral_public = X25519Public::from(&ephemeral_secret);

    let shared = ephemeral_secret.diffie_hellman(device_pub);

    let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut key_material = [0u8; 64];
    hk.expand(b"TextSecure Provisioning Message", &mut key_material)
        .map_err(|e| SignalError::Other(format!("HKDF expand failed: {e}")))?;
    let aes_key = &key_material[..32];
    let mac_key = &key_material[32..];

    let plaintext = msg.encode_to_vec();
    let mut iv = [0u8; 16];
    rng.fill_bytes(&mut iv);

    type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
    let cipher =
        Aes256CbcEnc::new_from_slices(aes_key, &iv).map_err(|e| SignalError::Other(e.to_string()))?;
    let ciphertext = cipher.encrypt_padded_vec_mut::<Pkcs7>(&plaintext);

    let mut mac = Hmac::<Sha256>::new_from_slice(mac_key)
        .map_err(|e| SignalError::Other(e.to_string()))?;
    mac.update(&[VERSION]);
    mac.update(&iv);
    mac.update(&ciphertext);
    let mac_bytes = mac.finalize().into_bytes();

    let mut body = Vec::with_capacity(1 + 16 + ciphertext.len() + 32);
    body.push(VERSION);
    body.extend_from_slice(&iv);
    body.extend_from_slice(&ciphertext);
    body.extend_from_slice(&mac_bytes);

    Ok(ProvisionEnvelope {
        public_key: Some(djb_key(ephemeral_public.as_bytes())),
        body: Some(body),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_djb_key_prefix() {
        let key = [0u8; 32];
        let djb = djb_key(&key);
        assert_eq!(djb.len(), 33);
        assert_eq!(djb[0], 0x05);
    }

    #[test]
    fn test_xeddsa_sign_length() {
        let mut rng = StdRng::from_os_rng();
        let secret = StaticSecret::random_from_rng(Rng06Compat(&mut rng));
        let sig = xeddsa_sign(&secret, b"test message", &mut rng);
        assert_eq!(sig.len(), 64);
    }

    #[test]
    fn test_parse_tsdevice_uri() {
        let key_b64 = BASE64_STANDARD.encode([5u8; 33]);
        let uri = format!("tsdevice:/?uuid=abc-123&pub_key={key_b64}");
        let (uuid, key) = parse_device_uri(&uri).unwrap();
        assert_eq!(uuid, "abc-123");
        assert_eq!(key.len(), 33);
    }

    #[test]
    fn test_parse_sgnl_uri() {
        let key_b64 = BASE64_URL_SAFE_NO_PAD.encode([5u8; 33]);
        let uri = format!("sgnl://linkdevice?uuid=testid&pub_key={key_b64}");
        let (uuid, key) = parse_device_uri(&uri).unwrap();
        assert_eq!(uuid, "testid");
        assert_eq!(key, vec![5u8; 33]);
    }

    #[test]
    fn test_encrypt_provision_message_roundtrip() {
        let mut rng = StdRng::from_os_rng();
        let device_secret = StaticSecret::random_from_rng(Rng06Compat(&mut rng));
        let device_pub = X25519Public::from(&device_secret);

        let msg = ProvisionMessage {
            number: Some("+123456789".to_string()),
            provisioning_code: Some("code123".to_string()),
            ..Default::default()
        };

        let envelope = encrypt_provision_message(&msg, &device_pub, &mut rng).unwrap();
        assert!(envelope.public_key.is_some());
        let body = envelope.body.unwrap();
        assert!(!body.is_empty());
        assert_eq!(body[0], 1u8); // VERSION byte
    }

    #[test]
    fn test_random_password_length() {
        let mut rng = StdRng::from_os_rng();
        let pw = random_password(&mut rng);
        assert_eq!(pw.len(), 28);
    }

    #[test]
    fn test_parse_uuid_bytes() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let bytes = parse_uuid_bytes(uuid).unwrap();
        assert_eq!(bytes.len(), 16);
        assert_eq!(bytes[0], 0x55);
        assert_eq!(bytes[1], 0x0e);
    }

    #[test]
    fn test_parse_uuid_bytes_invalid() {
        assert!(parse_uuid_bytes("not-a-uuid").is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_libsignal_session_and_encrypt() {
        // Verify we can establish a session and encrypt a message using libsignal-protocol
        let mut rng = StdRng::from_os_rng();

        let sender_identity = IdentityKeyPair::generate(&mut rng);
        let receiver_identity = IdentityKeyPair::generate(&mut rng);
        let receiver_address = ProtocolAddress::new(
            "receiver".to_string(),
            DeviceId::try_from(2u32).unwrap(),
        );

        let spk_pair = sigprot::KeyPair::generate(&mut rng);
        let spk_sig = receiver_identity
            .private_key()
            .calculate_signature(&spk_pair.public_key.serialize(), &mut rng)
            .unwrap();

        let kyber_pair =
            sigprot::kem::KeyPair::generate(sigprot::kem::KeyType::Kyber1024, &mut rng);
        let kyber_sig = receiver_identity
            .private_key()
            .calculate_signature(&kyber_pair.public_key.serialize(), &mut rng)
            .unwrap();

        let mut store = InMemSignalProtocolStore::new(sender_identity, 1).unwrap();

        let bundle = PreKeyBundle::new(
            2,
            DeviceId::try_from(2u32).unwrap(),
            None,
            SignedPreKeyId::from(1),
            spk_pair.public_key,
            spk_sig.to_vec(),
            KyberPreKeyId::from(1),
            kyber_pair.public_key,
            kyber_sig.to_vec(),
            *receiver_identity.identity_key(),
        )
        .unwrap();

        sigprot::process_prekey_bundle(
            &receiver_address,
            &mut store.session_store,
            &mut store.identity_store,
            &bundle,
            std::time::SystemTime::now(),
            &mut rng,
        )
        .await
        .unwrap();

        let ct = sigprot::message_encrypt(
            b"test sync message",
            &receiver_address,
            &mut store.session_store,
            &mut store.identity_store,
            std::time::SystemTime::now(),
            &mut rng,
        )
        .await
        .unwrap();

        assert_eq!(ct.message_type(), sigprot::CiphertextMessageType::PreKey);
        assert!(!ct.serialize().is_empty());
    }
}

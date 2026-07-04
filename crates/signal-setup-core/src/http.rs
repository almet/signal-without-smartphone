//! This module talks to Signal's service endpoints over HTTPS.
//! This is thought as a replacement for using signal-cli, ditching the
//! Java runtime dependency.
//!
//! Registration flow:
//!   1. `request_verification_code` sends an SMS code via Signal's API.
//!   2. `verify_and_register` verifies the code and registers the account,
//!      generating cryptographic keys.
//!
//! Device-linking flows:
//!   3. `link_device` parses a `tsdevice://` (or `sgnl://linkdevice`) URI from
//!      Signal Desktop's QR code and provisions Desktop as a linked device via
//!      Signal's provisioning API, then sends the initial sync messages using
//!      libsignal-protocol.

use base64::prelude::*;
use libsignal_protocol::{self as sigprot, Aci, IdentityKey, IdentityKeyPair};
use prost::Message as ProstMessage;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rand::{Rng, RngCore};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use x25519_dalek::PublicKey as X25519Public;

use crate::crypto::{
    encrypt_profile_field, encrypt_provision_message, encrypt_with_libsignal, parse_uuid_bytes,
    pem_to_der, random_password, DevicePreKeyBundle,
};
use crate::proto::{
    ContentProto, ProvisionMessage, SyncBlockedProto, SyncContactsProto, SyncMsgProto,
};
use crate::types::{SignalAccount, SignalError, VerificationRequest};

const SIGNAL_PROD: &str = "https://chat.signal.org";
const SIGNAL_STAGING: &str = "https://chat.staging.signal.org";

static USE_STAGING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn enable_staging() {
    USE_STAGING.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn signal_api() -> &'static str {
    if USE_STAGING.load(std::sync::atomic::Ordering::Relaxed) {
        SIGNAL_STAGING
    } else {
        SIGNAL_PROD
    }
}

/// Ask Signal to start a registration session for `phone`.
///
/// Returns `VerificationRequest::CaptchaRequired` if Signal wants the user to
/// solve a captcha before it sends the SMS code.
pub fn request_verification_code(
    phone: &str,
    captcha: Option<&str>,
) -> Result<VerificationRequest, SignalError> {
    let client = build_client();

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
        .post(format!("{}/v1/verification/session", signal_api()))
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

    let session = if let Some(token) = captcha {
        patch_session_with_captcha(&client, &session.id, token)?
    } else {
        session
    };

    finalize_session_request(&client, session)
}

/// Submit a captcha token and retrieve the updated session.
pub fn submit_captcha(
    session_id: &str,
    captcha_token: &str,
) -> Result<VerificationRequest, SignalError> {
    let client = build_client();
    let session = patch_session_with_captcha(&client, session_id, captcha_token)?;
    finalize_session_request(&client, session)
}

/// Given a session, either return that captcha is still required or POST the
/// SMS code request and return `CodeSent`.
fn finalize_session_request(
    client: &Client,
    session: RegistrationSessionResponse,
) -> Result<VerificationRequest, SignalError> {
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
            "{}/v1/verification/session/{}/code",
            signal_api(),
            session.id
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

/// Verify the user-supplied `code` and register the account.
///
/// Generates fresh identity keys, signed pre-keys, and Kyber last-resort keys.
/// Submits them all to Signal's `/v1/registration` endpoint. Also publishes an
/// empty versioned profile, which the server requires before it will issue the
/// profile key credentials that group operations rely on.
///
/// `discoverable_by_phone_number` controls whether other Signal users can find
/// this account by searching for its phone number.
///
/// On success returns a `SignalAccount` that must be kept alive for the device-
/// linking step.
pub fn verify_and_register(
    phone: &str,
    session_id: &str,
    code: &str,
    skip_device_transfer: bool,
    discoverable_by_phone_number: bool,
) -> Result<SignalAccount, SignalError> {
    let client = build_client();
    let mut rng = StdRng::from_os_rng();

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
            "{}/v1/verification/session/{session_id}/code",
            signal_api()
        ))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .json(&SubmitCodeBody { code })
        .send()?;

    // 409 means the session is already verified (happens on retries after a
    // DeviceTransferAvailable response).
    let verified: SubmitCodeResponse = if resp.status().as_u16() == 409 {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        SubmitCodeResponse {
            verified: body
                .get("verified")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }
    } else {
        parse_response(resp)?
    };
    if !verified.verified {
        return Err(SignalError::Other(
            "Verification code was not accepted.".into(),
        ));
    }

    let password = random_password(&mut rng);
    let aci_identity = IdentityKeyPair::generate(&mut rng);
    let pni_identity = IdentityKeyPair::generate(&mut rng);

    // Curve25519 signed pre-keys.
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

    // Kyber-1024 last-resort pre-keys.
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

    let registration_id: u32 = rng.random_range(1..=16383);
    let pni_registration_id: u32 = rng.random_range(1..=16383);
    let mut unidentified_access_key = [0u8; 16];
    rng.fill_bytes(&mut unidentified_access_key);

    let mut master_key = vec![0u8; 32];
    rng.fill_bytes(&mut master_key);

    let mut profile_key = vec![0u8; 32];
    rng.fill_bytes(&mut profile_key);

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
        versioned_expiration_timer: bool,
        attachment_backfill: bool,
        spqr: bool,
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
            // Mirrors what signal-cli sends for a primary device.
            capabilities: Capabilities {
                storage: true,
                versioned_expiration_timer: true,
                attachment_backfill: false,
                spqr: true,
            },
            unidentified_access_key: BASE64_STANDARD.encode(unidentified_access_key),
            unrestricted_unidentified_access: false,
            discoverable_by_phone_number,
            name: String::new(),
        },
        skip_device_transfer,
        every_signed_key_valid: true,
        aci_identity_key: BASE64_STANDARD.encode(aci_identity.identity_key().serialize()),
        pni_identity_key: BASE64_STANDARD.encode(pni_identity.identity_key().serialize()),
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
        .post(format!("{}/v1/registration", signal_api()))
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

    // Publish an empty versioned profile. This is useful for group operations
    // (like creating groups and accepting invitations).
    if let Some(aci) = reg.aci.as_deref() {
        upload_versioned_profile(&client, aci, &password, &profile_key, &mut rng)?;
    }

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
        desktop_profile: None,
    })
}

fn upload_versioned_profile(
    client: &Client,
    aci_str: &str,
    password: &str,
    profile_key: &[u8],
    rng: &mut StdRng,
) -> Result<(), SignalError> {
    let aci = Aci::parse_from_service_id_string(aci_str)
        .ok_or_else(|| SignalError::Other(format!("Invalid ACI UUID: {aci_str}")))?;

    let key_bytes: [u8; 32] = profile_key
        .try_into()
        .map_err(|_| SignalError::Other("Profile key must be 32 bytes".into()))?;
    let key = zkgroup::profiles::ProfileKey::create(key_bytes);
    let commitment = key.get_commitment(aci);
    let version = key.get_profile_key_version(aci);

    // Padded plaintext lengths, matching Signal's ProfileCipher brackets.
    const NAME_LENGTHS: &[usize] = &[53, 257];
    const ABOUT_LENGTHS: &[usize] = &[128, 254, 512];
    const EMOJI_LENGTHS: &[usize] = &[32];

    let name = encrypt_profile_field(&key_bytes, b"", NAME_LENGTHS, rng)?;
    let about = encrypt_profile_field(&key_bytes, b"", ABOUT_LENGTHS, rng)?;
    let about_emoji = encrypt_profile_field(&key_bytes, b"", EMOJI_LENGTHS, rng)?;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ProfileWrite {
        /// Hex encoded profile key version, derived from the key and the ACI.
        version: String,
        name: String,
        about: String,
        about_emoji: String,
        avatar: bool,
        same_avatar: bool,
        commitment: String,
    }

    let body = ProfileWrite {
        version: version.as_ref().to_string(),
        name: BASE64_STANDARD.encode(&name),
        about: BASE64_STANDARD.encode(&about),
        about_emoji: BASE64_STANDARD.encode(&about_emoji),
        avatar: false,
        same_avatar: false,
        commitment: BASE64_STANDARD.encode(zkgroup::serialize(&commitment)),
    };

    let resp = client
        .put(format!("{}/v1/profile", signal_api()))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .basic_auth(aci_str, Some(password))
        .json(&body)
        .send()?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(SignalError::Api {
            status: status.as_u16(),
            body,
        });
    }
    Ok(())
}

/// Link Signal Desktop as a secondary device using a `tsdevice://` or
/// `sgnl://linkdevice` URI decoded from its QR code.
///
/// The `account` must come from a successful `verify_and_register` call.
pub fn link_device(account: &SignalAccount, device_uri: &str) -> Result<(), SignalError> {
    let client = build_client();
    let mut rng = StdRng::from_os_rng();

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

    // Obtain a provisioning code from Signal.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DeviceCode {
        verification_code: String,
    }

    let resp = client
        .get(format!("{}/v1/devices/provisioning/code", signal_api()))
        .header("X-Signal-Agent", "OWD")
        .basic_auth(aci, Some(&account.password))
        .send()?;
    let code: DeviceCode = parse_response(resp)?;

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

    // Send the encrypted envelope to Signal's provisioning endpoint.
    #[derive(Serialize)]
    struct SendEnvelope {
        body: String,
    }

    let envelope_bytes = envelope.encode_to_vec();
    let resp = client
        .put(format!("{}/v1/provisioning/{ephemeral_id}", signal_api()))
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

    // Signal Desktop waits for sync messages from device 1 after linking.
    // We send a SyncMessage{contacts.isComplete=true, blocked={}} so Desktop
    // knows sync is complete and transitions to the main screen.
    // We ignore errors here: Desktop will eventually time out gracefully.
    let _ = send_linked_device_sync(&client, account, &mut rng);

    Ok(())
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
            .get(format!("{}/v2/keys/{aci}/{device_id}", signal_api()))
            .header("X-Signal-Agent", "OWD")
            .basic_auth(aci, Some(&account.password))
            .send()?;

        if resp.status().is_success() {
            let kr: KeyResponse = resp.json()?;
            if let Some(dev) = kr.devices.into_iter().next() {
                let identity_key_bytes = BASE64_STANDARD
                    .decode(&kr.identity_key)
                    .map_err(|e| SignalError::Other(format!("bad identity key: {e}")))?;
                let identity_key = IdentityKey::decode(&identity_key_bytes)
                    .map_err(|e| SignalError::Other(format!("decode identity key: {e}")))?;
                let signed_prekey_bytes = BASE64_STANDARD
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
                        let raw = BASE64_STANDARD
                            .decode(&pk.public_key)
                            .map_err(|e| SignalError::Other(format!("bad opk: {e}")))?;
                        (Some(pk.key_id), Some(raw))
                    }
                    None => (None, None),
                };
                let (kyber_id, kyber_bytes, kyber_sig) = match dev.pq_pre_key {
                    Some(pq) => {
                        let key_bytes = BASE64_STANDARD
                            .decode(&pq.public_key)
                            .map_err(|e| SignalError::Other(format!("bad pq key: {e}")))?;
                        let sig_bytes = BASE64_STANDARD
                            .decode(&pq.signature)
                            .map_err(|e| SignalError::Other(format!("bad pq sig: {e}")))?;
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

    let wire =
        rt.block_on(async { encrypt_with_libsignal(&plaintext, account, &bundle, rng).await })?;

    // Send via Signal's message endpoint (device 1 to device 2, same account).
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
        .put(format!("{}/v1/messages/{aci}", signal_api()))
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

/// Build a `reqwest` blocking client.
///
/// In production mode the client pins Signal's leaf certificate. In staging
/// mode it trusts only the Signal root CA (the staging server presents a
/// different leaf certificate so leaf pinning would fail).
fn build_client() -> Client {
    let timeout = std::time::Duration::from_secs(30);

    let tls_config = if USE_STAGING.load(std::sync::atomic::Ordering::Relaxed) {
        build_staging_tls_config()
    } else {
        build_production_tls_config()
    };

    Client::builder()
        .use_preconfigured_tls(tls_config)
        .timeout(timeout)
        .build()
        .expect("Failed to build HTTP client")
}

fn build_staging_tls_config() -> rustls::ClientConfig {
    use rustls::pki_types::CertificateDer;
    use std::sync::Arc;

    let ca_der = pem_to_der(include_str!("../signal-root-ca.crt"));
    let mut root_store = rustls::RootCertStore::empty();
    root_store
        .add(CertificateDer::from(ca_der))
        .expect("Failed to add Signal root CA");

    rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("Failed to set TLS protocol versions")
        .with_root_certificates(root_store)
        .with_no_client_auth()
}

fn build_production_tls_config() -> rustls::ClientConfig {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
    use std::sync::Arc;

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

    let pinned_der = pem_to_der(include_str!("../signal-root.crt"));

    rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("Failed to set TLS protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedCertVerifier { pinned_der }))
        .with_no_client_auth()
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
            "{}/v1/verification/session/{session_id}",
            signal_api()
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
            let (k, v) = kv.split_once('=')?;
            Some((k.to_string(), percent_decode(v)))
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Smoke test: hit Signal's production server with a real HTTPS request
    /// and confirm the client can complete the round-trip: TLS handshake
    /// against the pinned leaf certificate, request framing, and JSON
    /// response parsing.
    ///
    /// A real network/TLS failure (`SignalError::Http`) fails the test. Any
    /// other outcome (a session response, a captcha challenge, or an API
    /// rejection) proves the client successfully talked to Signal.
    #[test]
    fn signal_production_roundtrip() {
        // E.164-shaped placeholder. Whether Signal accepts the session,
        // demands a captcha, or rejects the number, all three outcomes prove
        // the request reached the server and a response came back.
        let result = request_verification_code("+15555550123", None);

        match result {
            Ok(_) => {}
            Err(SignalError::Api { .. }) => {}
            Err(SignalError::CaptchaRequired) => {}
            Err(SignalError::Other(_)) => {}
            Err(SignalError::Http(e)) => {
                panic!("network/TLS failure talking to Signal production: {e}");
            }
            Err(e) => panic!("unexpected error from Signal production: {e}"),
        }
    }

    /// Reproduces the user-reported failure with a real toll-free-shaped
    /// number against production. Captures the full chain of source errors
    /// so we can see what's actually going wrong at the transport layer
    /// (DNS, TCP, TLS, timeout, etc.).
    #[test]
    fn signal_production_roundtrip_reported_number() {
        let result = request_verification_code("+18777804236", None);

        match result {
            Ok(_) => {}
            Err(SignalError::Api { .. }) => {}
            Err(SignalError::CaptchaRequired) => {}
            Err(SignalError::Other(_)) => {}
            Err(SignalError::Http(e)) => {
                let mut chain = format!("{e}");
                let mut src: &dyn std::error::Error = &e;
                while let Some(next) = src.source() {
                    chain.push_str(&format!("\n  caused by: {next}"));
                    src = next;
                }
                panic!("network/TLS failure talking to Signal production:\n{chain}");
            }
            Err(e) => panic!("unexpected error from Signal production: {e}"),
        }
    }
}

/// The result of setting a username on an account.
#[derive(Debug, Clone)]
pub struct UsernameConfirmation {
    /// The confirmed username, e.g. `username.1234`.
    pub username: String,
    /// Server handle for the username link (used to build a `sgnl.link` URL).
    pub username_link_handle: Option<String>,
    /// The 32-byte entropy the link ciphertext was derived from; needed to
    /// reconstruct the shareable username link.
    pub username_link_entropy: [u8; 32],
}

/// Reserve and confirm a Signal `nickname.discriminator` username for an
/// already-registered account.
pub fn set_username(
    account: &SignalAccount,
    username: &str,
) -> Result<UsernameConfirmation, SignalError> {
    use usernames::{create_for_username, Username};

    let aci = account
        .aci
        .as_deref()
        .ok_or_else(|| SignalError::Other("Account has no ACI; cannot set a username".into()))?;

    let parsed = Username::new(username).map_err(|e| {
        SignalError::Other(format!(
            "Invalid username '{username}' (expected nickname.discriminator, e.g. name.1234): {e:?}"
        ))
    })?;

    let client = build_client();
    let hash = parsed.hash();
    let hash_b64 = BASE64_URL_SAFE_NO_PAD.encode(hash);

    // 1. Reserve the hash. The server holds it for 5 minutes.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ReserveBody {
        username_hashes: Vec<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ReserveResponse {
        username_hash: String,
    }

    let resp = client
        .put(format!(
            "{}/v1/accounts/username_hash/reserve",
            signal_api()
        ))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .basic_auth(aci, Some(&account.password))
        .json(&ReserveBody {
            username_hashes: vec![hash_b64.clone()],
        })
        .send()?;
    let reserved: ReserveResponse = parse_response(resp)?;

    // 2. Confirm it with the zk-proof and an encrypted username link.
    let mut rng = StdRng::from_os_rng();
    let mut randomness = [0u8; 32];
    rng.fill_bytes(&mut randomness);
    let proof = parsed
        .proof(&randomness)
        .map_err(|e| SignalError::Other(format!("username proof: {e:?}")))?;
    let (entropy, encrypted_username) =
        create_for_username(&mut rng, username.to_string(), None)
            .map_err(|e| SignalError::Other(format!("username link: {e:?}")))?;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ConfirmBody {
        username_hash: String,
        zk_proof: String,
        encrypted_username: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ConfirmResponse {
        #[serde(default)]
        username_link_handle: Option<String>,
    }

    let resp = client
        .put(format!(
            "{}/v1/accounts/username_hash/confirm",
            signal_api()
        ))
        .header("Content-Type", "application/json")
        .header("X-Signal-Agent", "OWD")
        .basic_auth(aci, Some(&account.password))
        .json(&ConfirmBody {
            username_hash: reserved.username_hash,
            zk_proof: BASE64_URL_SAFE_NO_PAD.encode(&proof),
            encrypted_username: BASE64_URL_SAFE_NO_PAD.encode(&encrypted_username),
        })
        .send()?;
    let confirmed: ConfirmResponse = parse_response(resp)?;

    Ok(UsernameConfirmation {
        username: username.to_string(),
        username_link_handle: confirmed.username_link_handle,
        username_link_entropy: entropy,
    })
}

/// A TURN/STUN relay server for calls, as returned by Signal's calling relay
/// endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnServer {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub urls_with_ips: Vec<String>,
}

/// Fetch the account's 1:1 calling relay (TURN/STUN) servers from Signal.
///
/// Without these, ICE cannot traverse NAT and calls never connect (they stay
/// "ringing" forever). Authenticated with the account's ACI + password.
pub fn fetch_turn_servers(account: &SignalAccount) -> Result<Vec<TurnServer>, SignalError> {
    let aci = account
        .aci
        .as_deref()
        .ok_or_else(|| SignalError::Other("Account has no ACI; cannot fetch relays".into()))?;

    let client = build_client();
    let resp = client
        .get(format!("{}/v2/calling/relays", signal_api()))
        .header("X-Signal-Agent", "OWD")
        .basic_auth(aci, Some(&account.password))
        .send()?;

    #[derive(Deserialize)]
    struct RelaysResponse {
        #[serde(default)]
        relays: Vec<TurnServer>,
    }

    let parsed: RelaysResponse = parse_response(resp)?;
    Ok(parsed.relays)
}

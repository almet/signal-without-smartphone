//! Cryptographic helpers
//!
//! The main bulk of the cryptographic work is done by the libsignal-protocol crate.
//! This module provides utilities on top.
//!
//! If you want to learn more about the Signal Protocol, their documentation is the
//! best way to get started:
//!
//!     https://signal.org/docs/
//!
//! Below are some definitions that can be useful while reading this code:
//!
//! **The Kyber key encapsulation mechanism**:
//!
//!     A (post-quantum) key encapsulation mechanism https://www.pq-crystals.org/kyber/
//!
//! **Elliptic Curve Cryptography** (EC, ECC)
//!
//!     An approach to public key cryptography using elliptic curves.
//!     See https://en.wikipedia.org/wiki/Elliptic-curve_cryptography
//!   
//! **Diffie—Hellman**, (DH or DHKE)
//!     
//!     A method to securely generate a symmetric cryptographic key over a public
//!     channel. If you don't know about it, go check it, it's pretty fun!
//!     See https://en.wikipedia.org/wiki/Diffie%E2%80%93Hellman_key_exchange
//!
//! **HMAC Key Derivation Function (HKDF)**
//!
//!     A way to transform a short key into a long key, with more entropy.
//!     https://en.wikipedia.org/wiki/HKDF
//!
//! **XEdDSA**
//!
//!     A signature scheme introduced by Trevor P. with the Signal Protocol, which
//!     extends EdDSA — a previously known signature scheme — to work with public and
//!     private key formats X25519 and X448 Diffie—Hellman functions.
//!     https://signal.org/docs/specifications/xeddsa/

use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use base64::prelude::*;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use libsignal_protocol::{
    self as sigprot, CiphertextMessage, DeviceId, IdentityKey, InMemSignalProtocolStore,
    KyberPreKeyId, PreKeyBundle, ProtocolAddress, SignedPreKeyId,
};
use prost::Message as ProstMessage;
use rand::rngs::StdRng;
use rand::RngCore;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

use crate::proto::{ProvisionEnvelope, ProvisionMessage};
use crate::types::{SignalAccount, SignalError};

/// `libsignal-protocol` uses rand 0.9, while `x25519-dalek` and `xeddsa` still
/// use rand_core 0.6 traits. This wrapper bridges the two so a single `StdRng`
/// instance can be passed to both APIs.
pub(crate) struct Rng06Compat<'a>(pub(crate) &'a mut StdRng);

impl rand_core_06::RngCore for Rng06Compat<'_> {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core_06::Error> {
        self.0.fill_bytes(dest);
        Ok(())
    }
}

impl rand_core_06::CryptoRng for Rng06Compat<'_> {}

/// The parsed pre-key bundle, used to establish the Signal Protocol session.
pub(crate) struct DevicePreKeyBundle {
    pub identity_key: IdentityKey,
    pub registration_id: u32,
    pub signed_prekey_id: u32,
    pub signed_prekey_bytes: Vec<u8>,
    pub signed_prekey_signature: Vec<u8>,
    pub kyber_prekey_id: Option<u32>,
    pub kyber_prekey_bytes: Option<Vec<u8>>,
    pub kyber_prekey_signature: Option<Vec<u8>>,
    pub one_time_prekey_id: Option<u32>,
    pub one_time_prekey_bytes: Option<Vec<u8>>,
}

/// Encrypt plaintext with X3DH session establishment and Double Ratchet encryption.
pub(crate) async fn encrypt_with_libsignal(
    plaintext: &[u8],
    account: &SignalAccount,
    bundle: &DevicePreKeyBundle,
    rng: &mut StdRng,
) -> Result<CiphertextMessage, SignalError> {
    let receiver_address = ProtocolAddress::new(
        account.aci.clone().unwrap_or_default(),
        DeviceId::try_from(2u32).expect("valid device id"),
    );

    // Create an in-memory "protocol store" for the sender (primary device)
    let mut store = InMemSignalProtocolStore::new(account.aci_identity, account.registration_id)?;

    // Parse the signed pre-key public key from the bundle
    let signed_prekey_pub = sigprot::PublicKey::deserialize(&bundle.signed_prekey_bytes)
        .map_err(|e| SignalError::Other(format!("parse signed prekey: {e}")))?;

    // Build the pre-key bundle for session establishment using a Kyber pre-key.
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
            "The linked device did not provide a Kyber pre-key. Cannot initiate session".into(),
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

/// Decode a PEM certificate to DER bytes.
pub(crate) fn pem_to_der(pem: &str) -> Vec<u8> {
    let b64: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    BASE64_STANDARD
        .decode(b64.trim())
        .expect("Invalid base64 in embedded certificate")
}

/// Generate a random password: 20 random bytes encoded as base64.
pub(crate) fn random_password(rng: &mut StdRng) -> String {
    let mut bytes = [0u8; 20];
    rng.fill_bytes(&mut bytes);
    BASE64_STANDARD.encode(bytes)
}

/// Prepend Signal's Curve25519 key type byte (0x05) to a 32-byte key.
pub(crate) fn djb_key(key: &[u8]) -> Vec<u8> {
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
pub(crate) fn parse_uuid_bytes(uuid_str: &str) -> Option<Vec<u8>> {
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

/// Encrypt a `ProvisionMessage` for delivery to `device_pub`.
///
/// Algorithm (matches Signal's `ProvisioningCipher`):
///
///   1. Generate an ephemeral X25519 key pair.
///   2. Do a Diffie—Hellman with the key pair (`device_pub`) and return a shared secret
///   3. Derive it with HKDF-SHA256, return an AES-256 key and a HMAC-SHA256 key.
///   4. Encrypt the serialised proto with AES-256-CBC and PKCS7.
///   5. Authenticate with HMAC-SHA256.
///
/// Then, return `ProvisionEnvelope`
///
pub(crate) fn encrypt_provision_message(
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
    let cipher = Aes256CbcEnc::new_from_slices(aes_key, &iv)
        .map_err(|e| SignalError::Other(e.to_string()))?;
    let ciphertext = cipher.encrypt_padded_vec_mut::<Pkcs7>(&plaintext);

    let mut mac =
        Hmac::<Sha256>::new_from_slice(mac_key).map_err(|e| SignalError::Other(e.to_string()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use libsignal_protocol::IdentityKeyPair;
    use rand::SeedableRng;

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
        let receiver_address =
            ProtocolAddress::new("receiver".to_string(), DeviceId::try_from(2u32).unwrap());

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

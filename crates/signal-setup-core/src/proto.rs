//! Inline protobuf wire types used by the registration and post-link sync
//! flows.
//!
//! Defining these `#[derive(ProstMessage)]` structs inline avoids a build.rs /
//! protoc dependency. Field numbers match the corresponding `.proto` files
//! from Signal's repository.

use prost::Message as ProstMessage;

// Inline protobuf types below avoid a build.rs / protoc dependency.

/// Signal's provisioning message, sent by a primary device to a secondary device.
/// Field numbers match `Provisioning.proto` from Signal's repository.
#[derive(Clone, PartialEq, ProstMessage)]
pub(crate) struct ProvisionMessage {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub aci_identity_key_public: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub aci_identity_key_private: Option<Vec<u8>>,
    #[prost(string, optional, tag = "3")]
    pub number: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub provisioning_code: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub user_agent: Option<String>,
    #[prost(bytes = "vec", optional, tag = "6")]
    pub profile_key: Option<Vec<u8>>,
    #[prost(bool, optional, tag = "7")]
    pub read_receipts: Option<bool>,
    #[prost(string, optional, tag = "8")]
    pub aci: Option<String>,
    #[prost(uint32, optional, tag = "9")]
    pub provisioning_version: Option<u32>,
    #[prost(string, optional, tag = "10")]
    pub pni: Option<String>,
    #[prost(bytes = "vec", optional, tag = "11")]
    pub pni_identity_key_public: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "12")]
    pub pni_identity_key_private: Option<Vec<u8>>,
    /// Deprecated in newer Signal versions, but still required by Signal Desktop
    /// when linking: without it Desktop throws and refuses to complete provisioning.
    #[prost(bytes = "vec", optional, tag = "13")]
    pub master_key: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "17")]
    pub aci_binary: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "18")]
    pub pni_binary: Option<Vec<u8>>,
}

/// Envelope wrapping an encrypted `ProvisionMessage`.
#[derive(Clone, PartialEq, ProstMessage)]
pub(crate) struct ProvisionEnvelope {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub public_key: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub body: Option<Vec<u8>>,
}

// Signal Protocol wire types used for the proactive post-link sync below.

/// Plaintext content wrapper before Signal Protocol encryption (SignalService.proto).
#[derive(Clone, PartialEq, ProstMessage)]
pub(crate) struct ContentProto {
    #[prost(message, optional, tag = "2")]
    pub sync_message: Option<SyncMsgProto>,
}

/// Minimal SyncMessage: contacts.isComplete = true and an empty blocked list
/// tells Signal Desktop that sync is done and there are no existing contacts.
#[derive(Clone, PartialEq, ProstMessage)]
pub(crate) struct SyncMsgProto {
    #[prost(message, optional, tag = "1")]
    pub contacts: Option<SyncContactsProto>,
    #[prost(message, optional, tag = "4")]
    pub blocked: Option<SyncBlockedProto>,
}

#[derive(Clone, PartialEq, ProstMessage)]
pub(crate) struct SyncContactsProto {
    /// `true` means "I've sent all my contacts (there are none)."
    #[prost(bool, optional, tag = "6")]
    pub is_complete: Option<bool>,
}

/// Empty blocked list.
#[derive(Clone, PartialEq, ProstMessage)]
pub(crate) struct SyncBlockedProto {}

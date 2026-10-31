//! Pair a device with a one-time code.
//!
//! The holder listens only while the session is alive. The joiner checks the
//! certificate fingerprint, then both sides run SPAKE2. The approval screen
//! runs after that. A denial sends no grant. The recovery password is not a
//! field on the grant or the announcement.

mod announce;
mod code;
mod error;
mod protocol;
mod qr;
mod tls;
mod util;

pub use announce::{Invite, SERVICE_TYPE, announcement_record, is_local_multicast_interface};
pub use code::{format_code, generate_code, parse_code};
pub use error::Error;
pub use protocol::{
    ApprovalRequest, DEFAULT_TIMEOUT, Decision, DeviceSecrets, JoinRequest, ListenConfig, Offer,
    PairingOutcome, PairingSession, join,
};
pub use qr::invite_qr_png;

/// Failure from pairing. Variants stay free of secret text.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    /// The typed code is not six digits.
    #[error("pairing code must be 6 digits")]
    Code,

    /// SPAKE2 confirmation failed, so the code was wrong.
    #[error("the pairing code was rejected")]
    CodeRejected,

    /// The TLS certificate is not the one that was announced.
    #[error("the certificate fingerprint does not match")]
    Fingerprint,

    /// Nobody finished the attempt before the deadline.
    #[error("pairing timed out")]
    TimedOut,

    /// The holder closed the attempt without a grant.
    #[error("the holder denied the request")]
    Denied,

    /// The session is gone.
    #[error("pairing closed")]
    Closed,

    /// The peer spoke a different protocol version.
    #[error("protocol version is not supported")]
    Version,

    /// A frame was empty, too large, or not valid for this step.
    #[error("the pairing message is invalid")]
    Message,

    /// The socket failed.
    #[error("network error")]
    Network,

    /// The TLS handshake failed for a reason other than the fingerprint.
    #[error("tls error")]
    Tls,
}

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use rcgen::{CertificateParams, KeyPair};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, ServerConfig, ServerConnection,
    SignatureScheme, StreamOwned,
};
use sha2::{Digest, Sha256};

use crate::Error;
use crate::util::hex_encode;

pub(crate) struct SessionCert {
    pub(crate) fingerprint: String,
    config: Arc<ServerConfig>,
}

pub(crate) fn generate_cert() -> Result<SessionCert, Error> {
    let key_pair = KeyPair::generate().map_err(|_| Error::Tls)?;
    let params = CertificateParams::new(vec!["satchel".to_string()]).map_err(|_| Error::Tls)?;
    let cert = params.self_signed(&key_pair).map_err(|_| Error::Tls)?;
    let der = cert.der().clone();
    let digest = Sha256::digest(der.as_ref());
    let fingerprint = hex_encode(digest.as_slice());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));
    let provider = crypto_provider();
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| Error::Tls)?
        .with_no_client_auth()
        .with_single_cert(vec![der], key)
        .map_err(|_| Error::Tls)?;
    Ok(SessionCert {
        fingerprint,
        config: Arc::new(config),
    })
}

pub(crate) fn server_stream(
    sock: TcpStream,
    cert: &SessionCert,
) -> Result<StreamOwned<ServerConnection, TcpStream>, Error> {
    let connection = ServerConnection::new(Arc::clone(&cert.config)).map_err(|_| Error::Tls)?;
    Ok(StreamOwned::new(connection, sock))
}

pub(crate) struct ClientHandshake {
    pub(crate) stream: StreamOwned<ClientConnection, TcpStream>,
    pub(crate) mismatched: Arc<AtomicBool>,
}

pub(crate) fn client_stream(
    sock: TcpStream,
    fingerprint_hex: &str,
) -> Result<ClientHandshake, Error> {
    let expected = crate::util::hex_decode_32(fingerprint_hex)?;
    let provider = crypto_provider();
    let mismatched = Arc::new(AtomicBool::new(false));
    let verifier = Arc::new(FingerprintVerifier {
        expected,
        algorithms: provider.signature_verification_algorithms,
        mismatched: Arc::clone(&mismatched),
    });
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| Error::Tls)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::clone(&verifier) as Arc<dyn ServerCertVerifier>)
        .with_no_client_auth();
    let name = ServerName::try_from("satchel").map_err(|_| Error::Tls)?;
    let connection = ClientConnection::new(Arc::new(config), name).map_err(|_| Error::Tls)?;
    Ok(ClientHandshake {
        stream: StreamOwned::new(connection, sock),
        mismatched,
    })
}

pub(crate) fn map_client(mismatched: &AtomicBool, err: std::io::Error) -> Error {
    if mismatched.load(Ordering::SeqCst) {
        return Error::Fingerprint;
    }
    map_io(err)
}

fn crypto_provider() -> Arc<rustls::crypto::CryptoProvider> {
    static PROVIDER: OnceLock<Arc<rustls::crypto::CryptoProvider>> = OnceLock::new();
    Arc::clone(PROVIDER.get_or_init(|| {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        // install_default takes ownership. A provider may already be installed.
        let _ = rustls::crypto::CryptoProvider::clone(&provider).install_default();
        provider
    }))
}

pub(crate) fn map_io(err: std::io::Error) -> Error {
    match err.kind() {
        std::io::ErrorKind::UnexpectedEof
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::ConnectionAborted
        | std::io::ErrorKind::BrokenPipe => Error::Closed,
        std::io::ErrorKind::TimedOut => Error::TimedOut,
        _ => Error::Network,
    }
}

struct FingerprintVerifier {
    expected: [u8; 32],
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
    mismatched: Arc<AtomicBool>,
}

impl std::fmt::Debug for FingerprintVerifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FingerprintVerifier")
            .field("mismatched", &self.mismatched.load(Ordering::SeqCst))
            .finish()
    }
}

impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let digest = Sha256::digest(end_entity.as_ref());
        if digest.as_slice() == self.expected.as_slice() {
            Ok(ServerCertVerified::assertion())
        } else {
            self.mismatched.store(true, Ordering::SeqCst);
            Err(rustls::Error::General("fingerprint mismatch".into()))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use crate::Error;

/// The two secrets the OS keystore holds for this profile.
///
/// The device key password is created at approval time and is not one of these.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Account {
    /// Opens the repository with official restic. It is never put in an invite.
    RecoveryPassword,
    /// Lets a device use the bucket. It is not the recovery password.
    BucketCredential,
}

impl Account {
    fn name(self) -> &'static str {
        match self {
            Self::RecoveryPassword => "recovery-password",
            Self::BucketCredential => "bucket-credential",
        }
    }
}

/// Store and load one account. Implementations must not log the secret.
pub trait SecretStore {
    /// Replace the value for `account`.
    fn put(&self, account: Account, secret: &str) -> Result<(), Error>;
    /// Return the stored value, or `None` when the account is absent.
    fn get(&self, account: Account) -> Result<Option<String>, Error>;
    /// Remove the account. Missing accounts are not an error.
    fn delete(&self, account: Account) -> Result<(), Error>;
}

/// In-memory store for tests and for a process that has no keystore.
#[derive(Debug, Default)]
pub struct MemoryStore {
    values: Mutex<BTreeMap<String, String>>,
}

impl SecretStore for MemoryStore {
    fn put(&self, account: Account, secret: &str) -> Result<(), Error> {
        reject_empty(secret)?;
        lock(&self.values).insert(account.name().to_string(), secret.to_string());
        Ok(())
    }

    fn get(&self, account: Account) -> Result<Option<String>, Error> {
        Ok(lock(&self.values).get(account.name()).cloned())
    }

    fn delete(&self, account: Account) -> Result<(), Error> {
        lock(&self.values).remove(account.name());
        Ok(())
    }
}

/// OS keystore entry. On Linux this talks to the Secret Service.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    /// The service name the installed app uses.
    pub fn system() -> Self {
        Self {
            service: "satchel".to_string(),
        }
    }

    /// A private service name so a test does not write the user's real entries.
    pub fn scoped(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

impl SecretStore for KeyringStore {
    fn put(&self, account: Account, secret: &str) -> Result<(), Error> {
        reject_empty(secret)?;
        let entry = keyring::Entry::new(&self.service, account.name()).map_err(store_error)?;
        entry.set_password(secret).map_err(store_error)?;
        Ok(())
    }

    fn get(&self, account: Account) -> Result<Option<String>, Error> {
        let entry = keyring::Entry::new(&self.service, account.name()).map_err(store_error)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(store_error(err)),
        }
    }

    fn delete(&self, account: Account) -> Result<(), Error> {
        let entry = keyring::Entry::new(&self.service, account.name()).map_err(store_error)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(store_error(err)),
        }
    }
}

fn reject_empty(secret: &str) -> Result<(), Error> {
    if secret.is_empty() {
        return Err(Error::Store { kind: "empty" });
    }
    Ok(())
}

fn store_error(err: keyring::Error) -> Error {
    let kind = match err {
        keyring::Error::PlatformFailure(_) => "platform",
        keyring::Error::NoStorageAccess(_) => "locked",
        keyring::Error::NoEntry => "missing",
        keyring::Error::BadEncoding(_) => "encoding",
        keyring::Error::TooLong(_, _) => "too-long",
        keyring::Error::Invalid(_, _) => "invalid",
        keyring::Error::Ambiguous(_) => "ambiguous",
        _ => "rejected",
    };
    Error::Store { kind }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

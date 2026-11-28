//! Token de acceso de Hugging Face que nunca se muestra ni se registra.
use std::fmt;
#[cfg(test)]
use std::sync::Mutex;

use keyring::Entry;

use super::error::HfError;

const PREFIX: &str = "hf_";
const MIN_BODY_LEN: usize = 30;
const REDACTED: &str = "hf_***";

#[derive(Clone)]
pub struct HfToken(String);

impl HfToken {
    /// Equivale a `^hf_[A-Za-z0-9]{30,}$` tras recortar los espacios.
    pub fn parse(raw: &str) -> Result<HfToken, HfError> {
        let trimmed = raw.trim();
        let valid = trimmed.strip_prefix(PREFIX).is_some_and(|body| {
            body.len() >= MIN_BODY_LEN && body.chars().all(|c| c.is_ascii_alphanumeric())
        });
        if valid {
            Ok(HfToken(trimmed.to_owned()))
        } else {
            Err(HfError::InvalidToken)
        }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HfToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl fmt::Display for HfToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// Persistencia del token del usuario.
pub trait SecretStore {
    fn get(&self) -> Result<Option<HfToken>, HfError>;
    fn set(&self, token: &HfToken) -> Result<(), HfError>;
    /// Borrar sin entrada previa no es un error.
    fn delete(&self) -> Result<(), HfError>;
}

const KEYRING_SERVICE: &str = "app-llama";
const KEYRING_ACCOUNT: &str = "huggingface-token";

/// Almacén en el llavero del sistema (Windows Credential Manager o Keychain).
pub struct KeyringStore;

impl KeyringStore {
    fn entry() -> Result<Entry, HfError> {
        Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(secret_error)
    }
}

impl SecretStore for KeyringStore {
    fn get(&self) -> Result<Option<HfToken>, HfError> {
        match Self::entry()?.get_password() {
            Ok(raw) => HfToken::parse(&raw).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(secret_error(error)),
        }
    }

    fn set(&self, token: &HfToken) -> Result<(), HfError> {
        Self::entry()?
            .set_password(token.expose())
            .map_err(secret_error)
    }

    fn delete(&self) -> Result<(), HfError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(secret_error(error)),
        }
    }
}

/// Los mensajes del llavero no incluyen el valor guardado.
fn secret_error(error: keyring::Error) -> HfError {
    HfError::Secret(error.to_string())
}

/// Almacén en memoria para los tests.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct MemoryStore(Mutex<Option<HfToken>>);

#[cfg(test)]
impl MemoryStore {
    pub(crate) fn with_token(token: HfToken) -> Self {
        MemoryStore(Mutex::new(Some(token)))
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<HfToken>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
impl SecretStore for MemoryStore {
    fn get(&self) -> Result<Option<HfToken>, HfError> {
        Ok(self.slot().clone())
    }

    fn set(&self, token: &HfToken) -> Result<(), HfError> {
        *self.slot() = Some(token.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), HfError> {
        *self.slot() = None;
        Ok(())
    }
}

/// Almacén que siempre falla, para probar la degradación.
#[cfg(test)]
pub(crate) struct FailingStore;

#[cfg(test)]
impl FailingStore {
    fn failure() -> HfError {
        HfError::Secret("almacén no disponible".to_owned())
    }
}

#[cfg(test)]
impl SecretStore for FailingStore {
    fn get(&self) -> Result<Option<HfToken>, HfError> {
        Err(Self::failure())
    }

    fn set(&self, _token: &HfToken) -> Result<(), HfError> {
        Err(Self::failure())
    }

    fn delete(&self) -> Result<(), HfError> {
        Err(Self::failure())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_token() -> String {
        format!("hf_{}", "a1B2".repeat(9))
    }

    fn assert_invalid(raw: &str) {
        assert!(matches!(HfToken::parse(raw), Err(HfError::InvalidToken)));
    }

    #[test]
    fn token_parse_valid() {
        assert!(HfToken::parse(&raw_token()).is_ok());
    }

    #[test]
    fn token_parse_trims_whitespace() {
        let token = HfToken::parse(&format!("  {}  \n", raw_token())).unwrap();
        assert_eq!(token.expose(), raw_token());
    }

    #[test]
    fn token_parse_wrong_prefix() {
        assert_invalid(&format!("ghp_{}", "a".repeat(34)));
    }

    #[test]
    fn token_parse_too_short() {
        assert_invalid("hf_abc");
    }

    #[test]
    fn token_parse_inner_space() {
        assert_invalid(&format!("hf_abc def{}", "a".repeat(30)));
    }

    #[test]
    fn token_parse_empty() {
        assert_invalid("");
    }

    #[test]
    fn token_debug_is_redacted() {
        let token = HfToken::parse(&raw_token()).unwrap();
        let shown = format!("{token:?}");
        assert_eq!(shown, "hf_***");
        assert!(!shown.contains(&raw_token()));
    }

    #[test]
    fn token_display_is_redacted() {
        let token = HfToken::parse(&raw_token()).unwrap();
        assert_eq!(format!("{token}"), "hf_***");
    }

    fn token() -> HfToken {
        HfToken::parse(&raw_token()).unwrap()
    }

    #[test]
    fn memory_store_roundtrip() {
        let store = MemoryStore::default();
        store.set(&token()).unwrap();
        let stored = store.get().unwrap().unwrap();
        assert_eq!(stored.expose(), raw_token());
    }

    #[test]
    fn memory_store_delete_absent_ok() {
        assert!(MemoryStore::default().delete().is_ok());
    }

    #[test]
    fn memory_store_delete_then_get() {
        let store = MemoryStore::default();
        store.set(&token()).unwrap();
        store.delete().unwrap();
        assert!(store.get().unwrap().is_none());
    }

    #[test]
    fn failing_store_always_returns_secret_error() {
        let store = FailingStore;
        assert!(matches!(store.get(), Err(HfError::Secret(_))));
        assert!(matches!(store.set(&token()), Err(HfError::Secret(_))));
        assert!(matches!(store.delete(), Err(HfError::Secret(_))));
    }

    #[test]
    fn memory_store_with_token_starts_filled() {
        let store = MemoryStore::with_token(token());
        assert!(store.get().unwrap().is_some());
    }
}

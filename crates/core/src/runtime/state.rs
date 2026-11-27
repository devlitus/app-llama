//! Estado persistido del runtime: qué llama-server está activo y qué limpiezas quedan pendientes.
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::runtime::{Backend, RuntimeError, RuntimeVersion};

const STATE_FILE: &str = "runtime.json";
const TEMP_FILE: &str = "runtime.json.tmp";
const BACKUP_FILE: &str = "runtime.json.bak";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeSource {
    Managed { tag: String },
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveRuntime {
    pub source: RuntimeSource,
    pub backend: Backend,
    pub server_path: PathBuf,
    pub version: RuntimeVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeState {
    pub schema: u32,
    pub active: Option<ActiveRuntime>,
    pub pending_cleanup: Vec<PathBuf>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            active: None,
            pending_cleanup: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RuntimeStatus {
    NotInstalled,
    Ready(ActiveRuntime),
    Broken { path: PathBuf },
}

pub fn status_of(state: &RuntimeState, exists: impl Fn(&Path) -> bool) -> RuntimeStatus {
    match &state.active {
        None => RuntimeStatus::NotInstalled,
        Some(active) if exists(&active.server_path) => RuntimeStatus::Ready(active.clone()),
        Some(active) => RuntimeStatus::Broken {
            path: active.server_path.clone(),
        },
    }
}

/// No es seguro para escrituras concurrentes: el fichero temporal tiene nombre fijo.
pub struct StateStore {
    runtime_dir: PathBuf,
}

impl StateStore {
    pub fn new(runtime_dir: impl Into<PathBuf>) -> Self {
        Self {
            runtime_dir: runtime_dir.into(),
        }
    }

    /// Un estado ausente equivale al estado inicial. Uno ilegible (no UTF-8, no parseable o de
    /// otro `schema`) también, pero se conserva como `.bak` para poder inspeccionarlo.
    /// Cualquier otro fallo de lectura (permisos, uso compartido) se devuelve como error sin
    /// tocar nada: tratarlo como estado inicial dejaría que un `save` posterior lo destruya.
    pub fn load(&self) -> Result<RuntimeState, RuntimeError> {
        let path = self.runtime_dir.join(STATE_FILE);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(RuntimeState::default()),
            Err(error) => return Err(error.into()),
        };
        match serde_json::from_slice::<RuntimeState>(&bytes) {
            Ok(state) if state.schema == SCHEMA_VERSION => Ok(state),
            _ => {
                self.back_up(&path);
                Ok(RuntimeState::default())
            }
        }
    }

    pub fn save(&self, state: &RuntimeState) -> Result<(), RuntimeError> {
        fs::create_dir_all(&self.runtime_dir)?;
        let json = serde_json::to_string_pretty(state).map_err(std::io::Error::from)?;
        let temp = self.runtime_dir.join(TEMP_FILE);
        fs::write(&temp, json)?;
        fs::rename(&temp, self.runtime_dir.join(STATE_FILE))?;
        Ok(())
    }

    // La copia es un auxilio de diagnóstico: si falla, arrancar limpio sigue siendo lo correcto.
    fn back_up(&self, path: &Path) {
        let _ = fs::copy(path, self.runtime_dir.join(BACKUP_FILE));
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::runtime::{Backend, CpuIsa, RuntimeVersion};

    fn version() -> RuntimeVersion {
        RuntimeVersion {
            build: 12780,
            commit: "abc1234".to_string(),
        }
    }

    fn managed() -> ActiveRuntime {
        ActiveRuntime {
            source: RuntimeSource::Managed {
                tag: "b12780".to_string(),
            },
            backend: Backend::Cuda {
                major: 13,
                minor: 5,
            },
            server_path: PathBuf::from("runtime")
                .join("b12780")
                .join("llama-server.exe"),
            version: version(),
        }
    }

    fn state_with(active: Option<ActiveRuntime>) -> RuntimeState {
        RuntimeState {
            active,
            ..RuntimeState::default()
        }
    }

    #[test]
    fn estado_roundtrip_gestionado() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path());
        let state = RuntimeState {
            pending_cleanup: vec![PathBuf::from("viejo")],
            ..state_with(Some(managed()))
        };
        store.save(&state).unwrap();
        assert_eq!(store.load().unwrap(), state);
    }

    #[test]
    fn estado_roundtrip_externo() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path());
        let active = ActiveRuntime {
            source: RuntimeSource::External,
            backend: Backend::Cpu { isa: CpuIsa::Avx2 },
            server_path: dir.path().join("Él dir").join("llama server"),
            version: version(),
        };
        let state = state_with(Some(active));
        store.save(&state).unwrap();
        assert_eq!(store.load().unwrap(), state);
    }

    #[test]
    fn estado_inexistente() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateStore::new(dir.path()).load().unwrap();
        assert_eq!(state.active, None);
        assert_eq!(state, RuntimeState::default());
    }

    #[test]
    fn estado_corrupto_hace_copia() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("runtime.json"), "{").unwrap();
        let state = StateStore::new(dir.path()).load().unwrap();
        assert_eq!(state, RuntimeState::default());
        let backup = std::fs::read_to_string(dir.path().join("runtime.json.bak")).unwrap();
        assert_eq!(backup, "{");
    }

    #[test]
    fn estado_no_utf8_hace_copia() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = [0xFF, 0xFE, b'{'];
        std::fs::write(dir.path().join("runtime.json"), bytes).unwrap();
        let state = StateStore::new(dir.path()).load().unwrap();
        assert_eq!(state, RuntimeState::default());
        let backup = std::fs::read(dir.path().join("runtime.json.bak")).unwrap();
        assert_eq!(backup, bytes);
    }

    #[test]
    fn estado_error_de_lectura_no_es_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("runtime.json")).unwrap();
        let result = StateStore::new(dir.path()).load();
        assert!(matches!(result, Err(RuntimeError::Io(_))));
        assert!(!dir.path().join("runtime.json.bak").exists());
    }

    #[test]
    fn estado_sobrescribe_existente() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path());
        store.save(&state_with(Some(managed()))).unwrap();
        let second = RuntimeState {
            pending_cleanup: vec![PathBuf::from("otro")],
            ..RuntimeState::default()
        };
        store.save(&second).unwrap();
        assert_eq!(store.load().unwrap(), second);
        assert!(!dir.path().join("runtime.json.tmp").exists());
    }

    #[test]
    fn estado_schema_desconocido() {
        let dir = tempfile::tempdir().unwrap();
        let json = r#"{"schema":99,"active":null,"pending_cleanup":[]}"#;
        std::fs::write(dir.path().join("runtime.json"), json).unwrap();
        let state = StateStore::new(dir.path()).load().unwrap();
        assert_eq!(state, RuntimeState::default());
        assert!(dir.path().join("runtime.json.bak").exists());
    }

    #[test]
    fn estado_guardado_atomico() {
        let dir = tempfile::tempdir().unwrap();
        StateStore::new(dir.path())
            .save(&RuntimeState::default())
            .unwrap();
        assert!(dir.path().join("runtime.json").exists());
        assert!(!dir.path().join("runtime.json.tmp").exists());
    }

    #[test]
    fn estado_crea_directorio() {
        let dir = tempfile::tempdir().unwrap();
        let runtime_dir = dir.path().join("a").join("runtime");
        StateStore::new(&runtime_dir)
            .save(&RuntimeState::default())
            .unwrap();
        assert!(runtime_dir.join("runtime.json").is_file());
    }

    #[test]
    fn status_no_instalado() {
        let status = status_of(&RuntimeState::default(), |_: &Path| true);
        assert_eq!(status, RuntimeStatus::NotInstalled);
    }

    #[test]
    fn status_listo() {
        let status = status_of(&state_with(Some(managed())), |_: &Path| true);
        assert_eq!(status, RuntimeStatus::Ready(managed()));
    }

    #[test]
    fn status_roto() {
        let status = status_of(&state_with(Some(managed())), |_: &Path| false);
        assert_eq!(
            status,
            RuntimeStatus::Broken {
                path: managed().server_path
            }
        );
    }
}

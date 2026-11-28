use std::io;
use std::path::Path;

use super::error::DownloadError;
use super::model::{FailureKind, FileSpec, FileStatus};

/// Espacio que debe sobrar tras descargar, para no dejar el disco al límite.
pub const SPACE_MARGIN: u64 = 1024 * 1024 * 1024;

/// Consulta del espacio libre, inyectable para que los tests no dependan del disco.
pub trait DiskSpace: Send + Sync {
    fn available(&self, path: &Path) -> io::Result<u64>;
}

/// Lee el espacio libre del volumen real que contiene `path`.
pub struct Fs4DiskSpace;

impl DiskSpace for Fs4DiskSpace {
    fn available(&self, path: &Path) -> io::Result<u64> {
        // El destino puede no existir aún: se pregunta por su primer ancestro existente.
        let existing = path.ancestors().find(|ancestor| ancestor.exists());
        fs4::available_space(existing.unwrap_or(path))
    }
}

/// Bytes que faltan por descargar: solo los ficheros pendientes, descontando
/// lo ya escrito en su `.part`. `part_lens` va alineado con `files`.
pub fn required_bytes(files: &[FileSpec], part_lens: &[u64]) -> u64 {
    files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.status == FileStatus::Pending)
        .map(|(index, file)| {
            let on_disk = part_lens.get(index).copied().unwrap_or(0);
            file.expected_size.saturating_sub(on_disk)
        })
        .fold(0, u64::saturating_add)
}

/// Falla si el volumen de `dest` no tiene sitio para `required` más el margen.
pub fn check_space(disk: &dyn DiskSpace, dest: &Path, required: u64) -> Result<(), DownloadError> {
    // El mensaje es fijo: el error de E/S original puede arrastrar rutas locales.
    let available = disk.available(dest).map_err(|_| {
        DownloadError::Failed(FailureKind::Io {
            message: "no se pudo consultar el espacio libre en disco".to_string(),
        })
    })?;
    let needed = required.saturating_add(SPACE_MARGIN);
    if available < needed {
        return Err(DownloadError::Failed(FailureKind::InsufficientSpace {
            needed,
            available,
        }));
    }
    Ok(())
}

#[cfg(test)]
pub struct FakeDiskSpace(pub Option<u64>);

#[cfg(test)]
impl DiskSpace for FakeDiskSpace {
    fn available(&self, _path: &Path) -> io::Result<u64> {
        self.0
            .ok_or_else(|| io::Error::other("fallo simulado de /secreto/ruta"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn file(size: u64, status: FileStatus) -> FileSpec {
        FileSpec {
            relative_path: "m.gguf".to_string(),
            url: "https://huggingface.co/x".to_string(),
            dest: "m.gguf".into(),
            expected_size: size,
            sha256: None,
            status,
        }
    }

    fn check(available: u64, required: u64) -> Result<(), DownloadError> {
        check_space(&FakeDiskSpace(Some(available)), Path::new("d"), required)
    }

    fn insufficient(result: Result<(), DownloadError>) -> Option<(u64, u64)> {
        match result {
            Err(DownloadError::Failed(FailureKind::InsufficientSpace { needed, available })) => {
                Some((needed, available))
            }
            _ => None,
        }
    }

    #[test]
    fn required_sin_parciales() {
        let files = vec![file(20 * GIB, FileStatus::Pending); 3];
        assert_eq!(required_bytes(&files, &[0, 0, 0]), 64_424_509_440);
    }

    #[test]
    fn required_descuenta_part() {
        let files = [file(60 * GIB, FileStatus::Pending)];
        assert_eq!(required_bytes(&files, &[10 * GIB]), 53_687_091_200);
    }

    #[test]
    fn required_ignora_verificados_y_preexistentes() {
        let files = [
            file(20 * GIB, FileStatus::Verified),
            file(20 * GIB, FileStatus::Preexisting),
        ];
        assert_eq!(required_bytes(&files, &[0, 0]), 0);
    }

    #[test]
    fn required_part_mayor_que_esperado_satura() {
        let files = [file(GIB, FileStatus::Pending)];
        assert_eq!(required_bytes(&files, &[2 * GIB]), 0);
    }

    #[test]
    fn espacio_insuficiente_60gb() {
        let result = check(40 * GIB, 60 * GIB);
        assert_eq!(insufficient(result), Some((65_498_251_264, 42_949_672_960)));
    }

    #[test]
    fn espacio_justo_con_margen() {
        assert!(check(10 * GIB + SPACE_MARGIN, 10 * GIB).is_ok());
    }

    #[test]
    fn espacio_justo_sin_margen() {
        assert!(insufficient(check(10 * GIB, 10 * GIB)).is_some());
    }

    #[test]
    fn necesario_satura_sin_desbordar() {
        assert!(insufficient(check(u64::MAX - 1, u64::MAX)).is_some());
    }

    #[test]
    fn error_de_consulta_no_filtra_la_ruta() {
        let result = check_space(&FakeDiskSpace(None), Path::new("d"), 0);
        match result {
            Err(DownloadError::Failed(FailureKind::Io { message })) => {
                assert!(!message.contains("secreto"));
            }
            other => panic!("resultado inesperado: {other:?}"),
        }
    }

    #[test]
    fn fs4_directorio_inexistente_usa_ancestro() {
        let tmp = tempfile::TempDir::new().unwrap();
        let missing = tmp.path().join("no").join("existe");
        let free = Fs4DiskSpace.available(&missing).unwrap();
        assert!(free > 0);
    }
}

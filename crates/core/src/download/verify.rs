//! Verificación de tamaño y SHA256 de los `.part` y renombrado final.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::error::DownloadError;
use super::model::{FailureKind, FileSpec, FileStatus};
use super::paths::part_path;

const BLOCK_SIZE: usize = 1024 * 1024;

/// SHA256 del fichero en hex minúsculas. El cálculo corre en un hilo de
/// bloqueo y comprueba la cancelación entre bloques de 1 MiB.
pub async fn sha256_file<F>(
    path: &Path,
    cancel: &CancellationToken,
    on_bytes: F,
) -> Result<String, DownloadError>
where
    F: FnMut(u64) + Send + 'static,
{
    digest_file(path, cancel, on_bytes).await.map(hex::encode)
}

/// Comprueba el tamaño y, si hay `sha256`, el hash. Con un tamaño distinto no
/// se hashea: el fichero ya es inválido.
pub async fn verify_file<F>(
    part: &Path,
    expected_size: u64,
    sha256: Option<&str>,
    cancel: &CancellationToken,
    on_bytes: F,
) -> Result<(), DownloadError>
where
    F: FnMut(u64) + Send + 'static,
{
    let expected_digest = sha256.map(parse_sha256).transpose()?;
    let actual = tokio::fs::metadata(part).await?.len();
    if actual != expected_size {
        return Err(DownloadError::Failed(FailureKind::SizeMismatch {
            expected: expected_size,
            actual,
        }));
    }
    let Some(expected_digest) = expected_digest else {
        return Ok(());
    };
    if digest_file(part, cancel, on_bytes).await? == expected_digest {
        Ok(())
    } else {
        Err(DownloadError::Failed(FailureKind::ChecksumMismatch))
    }
}

/// Exactamente 32 bytes en hex, sin distinguir mayúsculas.
fn parse_sha256(hex_digest: &str) -> Result<[u8; 32], DownloadError> {
    let mut digest = [0u8; 32];
    hex::decode_to_slice(hex_digest, &mut digest)
        .map_err(|_| DownloadError::InvalidRequest("sha256 no válido"))?;
    Ok(digest)
}

async fn digest_file<F>(
    path: &Path,
    cancel: &CancellationToken,
    on_bytes: F,
) -> Result<[u8; 32], DownloadError>
where
    F: FnMut(u64) + Send + 'static,
{
    let path = path.to_path_buf();
    let cancel = cancel.clone();
    tokio::task::spawn_blocking(move || digest_blocking(&path, &cancel, on_bytes))
        .await
        .map_err(|_| {
            DownloadError::Io(std::io::Error::other("el cálculo del hash se interrumpió"))
        })?
}

fn digest_blocking(
    path: &Path,
    cancel: &CancellationToken,
    mut on_bytes: impl FnMut(u64),
) -> Result<[u8; 32], DownloadError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut block = vec![0u8; BLOCK_SIZE];
    loop {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let read = file.read(&mut block)?;
        if read == 0 {
            return Ok(hasher.finalize().into());
        }
        hasher.update(&block[..read]);
        on_bytes(read as u64);
    }
}

/// Motivo de `InvalidRequest` cuando se finaliza una unidad con partes sin verificar.
pub(super) const UNVERIFIED_PART: &str = "parte sin verificar";

/// Si queda algo en `path`. Un error distinto de «no existe» se toma como que sí:
/// es la lectura prudente para quien decide qué borrar o dar por hecho.
fn present(path: &Path) -> bool {
    !matches!(std::fs::metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

fn has_final_of_expected_size(file: &FileSpec) -> bool {
    std::fs::metadata(&file.dest).is_ok_and(|metadata| metadata.len() == file.expected_size)
}

/// Una parte `Verified` cuyo `.part` ya es su final: sin `.part` y con el
/// destino del tamaño esperado. Es lo que deja una caída a mitad de `finalize_unit`.
pub(super) fn is_already_renamed(file: &FileSpec) -> bool {
    file.status == FileStatus::Verified
        && !present(&part_path(&file.dest))
        && has_final_of_expected_size(file)
}

/// Una parte `Verified` de la que no queda ni el `.part` ni su final, por
/// ejemplo porque el usuario los borró mientras la unidad estaba en pausa.
pub(super) fn is_lost(file: &FileSpec) -> bool {
    file.status == FileStatus::Verified
        && !present(&part_path(&file.dest))
        && !has_final_of_expected_size(file)
}

/// Renombra los `.part` verificados a su nombre final, en orden. Una parte
/// `Pending` se rechaza antes de tocar nada. Los destinos se comprueban antes
/// de renombrar y, si un renombrado falla, los ya hechos se revierten: nunca
/// queda a la vista un conjunto incompleto de `.gguf`. En Windows `rename`
/// sobreescribiría un destino existente, por eso la comprobación es explícita.
///
/// Es idempotente: las partes que ya están renombradas se saltan, así que una
/// unidad que se quedó a medias tras una caída se puede finalizar de nuevo.
pub fn finalize_unit(files: &[FileSpec]) -> Result<(), DownloadError> {
    if files.iter().any(|file| file.status == FileStatus::Pending) {
        return Err(DownloadError::InvalidRequest(UNVERIFIED_PART));
    }
    let (already, to_rename): (Vec<&FileSpec>, Vec<&FileSpec>) = files
        .iter()
        .filter(|file| file.status == FileStatus::Verified)
        .partition(|file| is_already_renamed(file));
    for file in &to_rename {
        if present(&part_path(&file.dest)) && file.dest.try_exists()? {
            return Err(DownloadError::DestinationExists);
        }
    }
    let mut renamed: Vec<&Path> = already.iter().map(|file| file.dest.as_path()).collect();
    for file in to_rename {
        if let Err(error) = std::fs::rename(part_path(&file.dest), &file.dest) {
            revert(&renamed);
            return Err(error.into());
        }
        renamed.push(&file.dest);
    }
    Ok(())
}

/// Devuelve cada fichero final a su `.part`. Es best-effort: ya se está
/// propagando el error que originó la reversión.
fn revert(renamed: &[&Path]) {
    for dest in renamed.iter().rev() {
        let _ = std::fs::rename(dest, part_path(dest));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::download::test_support::{file_spec, pattern_bytes};

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn write_file(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).expect("escribir");
        path
    }

    fn token() -> CancellationToken {
        CancellationToken::new()
    }

    async fn verify(path: &Path, size: u64, sha: Option<&str>) -> Result<(), DownloadError> {
        verify_file(path, size, sha, &token(), |_| {}).await
    }

    fn failure(result: Result<(), DownloadError>) -> FailureKind {
        match result {
            Err(DownloadError::Failed(kind)) => kind,
            other => panic!("se esperaba Failed(_), llegó {other:?}"),
        }
    }

    #[tokio::test]
    async fn sha256_vector_conocido() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", b"abc");

        let digest = sha256_file(&path, &token(), |_| {}).await.expect("hash");

        assert_eq!(digest, ABC_SHA256);
    }

    #[tokio::test]
    async fn sha256_fichero_vacio() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", b"");

        let digest = sha256_file(&path, &token(), |_| {}).await.expect("hash");

        assert_eq!(
            digest,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn sha256_mayor_que_bloque() {
        let body = pattern_bytes(3 * BLOCK_SIZE + 7);
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", &body);

        let digest = sha256_file(&path, &token(), |_| {}).await.expect("hash");

        assert_eq!(digest, hex::encode(Sha256::digest(&body)));
    }

    #[tokio::test]
    async fn verify_ok() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", b"abc");

        assert!(verify(&path, 3, Some(ABC_SHA256)).await.is_ok());
    }

    #[tokio::test]
    async fn verify_hash_mayusculas() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", b"abc");

        let upper = ABC_SHA256.to_uppercase();

        assert!(verify(&path, 3, Some(&upper)).await.is_ok());
    }

    #[tokio::test]
    async fn verify_tamano_distinto() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", b"ab");
        let calls = Arc::new(AtomicU64::new(0));
        let seen = Arc::clone(&calls);

        let result = verify_file(&path, 3, Some(ABC_SHA256), &token(), move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        })
        .await;

        assert_eq!(
            failure(result),
            FailureKind::SizeMismatch {
                expected: 3,
                actual: 2
            }
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn verify_hash_distinto() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", b"abd");

        let result = verify(&path, 3, Some(ABC_SHA256)).await;

        assert_eq!(failure(result), FailureKind::ChecksumMismatch);
    }

    #[tokio::test]
    async fn verify_sin_sha_solo_tamano() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", b"abc");

        assert!(verify(&path, 3, None).await.is_ok());
    }

    #[tokio::test]
    async fn verify_rechaza_hash_que_no_es_hex() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", b"abc");
        let not_hex = "z".repeat(64);

        let result = verify(&path, 3, Some(&not_hex)).await;

        assert!(matches!(result, Err(DownloadError::InvalidRequest(_))));
    }

    #[tokio::test]
    async fn verify_rechaza_hash_de_longitud_incorrecta() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", b"abc");
        let short = &ABC_SHA256[..62];
        let long = format!("{ABC_SHA256}00");

        for bad in [short, long.as_str(), ""] {
            let result = verify(&path, 3, Some(bad)).await;
            assert!(
                matches!(result, Err(DownloadError::InvalidRequest(_))),
                "{bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn sha256_cancelable() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = write_file(&dir, "f", &pattern_bytes(3 * BLOCK_SIZE));
        let cancel = token();
        let trigger = cancel.clone();

        let result = sha256_file(&path, &cancel, move |_| trigger.cancel()).await;

        assert!(matches!(result, Err(DownloadError::Cancelled)));
    }

    fn verified_unit(dir: &tempfile::TempDir, names: &[&str]) -> Vec<FileSpec> {
        names
            .iter()
            .map(|name| {
                let dest = dir.path().join(name);
                std::fs::write(part_path(&dest), name.as_bytes()).expect("escribir .part");
                FileSpec {
                    status: FileStatus::Verified,
                    ..file_spec(String::new(), &dest, name.len())
                }
            })
            .collect()
    }

    #[test]
    fn finalize_renombra_todas() {
        let dir = tempfile::tempdir().expect("tmp");
        let files = verified_unit(&dir, &["a.gguf", "b.gguf", "c.gguf"]);

        finalize_unit(&files).expect("finalizar");

        for file in &files {
            assert!(file.dest.is_file());
            assert!(!part_path(&file.dest).exists());
        }
    }

    #[test]
    fn finalize_no_sobrescribe() {
        let dir = tempfile::tempdir().expect("tmp");
        let files = verified_unit(&dir, &["a.gguf"]);
        std::fs::write(&files[0].dest, b"previo").expect("destino previo");

        let result = finalize_unit(&files);

        assert!(matches!(result, Err(DownloadError::DestinationExists)));
        assert_eq!(std::fs::read(&files[0].dest).expect("leer"), b"previo");
        assert!(part_path(&files[0].dest).is_file());
    }

    #[test]
    fn finalize_con_conflicto_no_renombra_ninguna() {
        let dir = tempfile::tempdir().expect("tmp");
        let files = verified_unit(&dir, &["a.gguf", "b.gguf"]);
        std::fs::write(&files[1].dest, b"previo").expect("destino previo");

        let result = finalize_unit(&files);

        assert!(matches!(result, Err(DownloadError::DestinationExists)));
        assert!(!files[0].dest.exists());
        assert!(part_path(&files[0].dest).is_file());
    }

    #[test]
    fn finalize_revierte_si_falla_un_renombrado() {
        let dir = tempfile::tempdir().expect("tmp");
        let files = verified_unit(&dir, &["a.gguf", "b.gguf"]);
        std::fs::remove_file(part_path(&files[1].dest)).expect("quitar .part");

        let result = finalize_unit(&files);

        assert!(matches!(result, Err(DownloadError::Io(_))));
        assert!(!files[0].dest.exists());
        assert!(part_path(&files[0].dest).is_file());
    }

    #[test]
    fn finalize_rechaza_parte_pendiente() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut files = verified_unit(&dir, &["a.gguf", "b.gguf", "c.gguf"]);
        files[1].status = FileStatus::Pending;

        let result = finalize_unit(&files);

        assert!(matches!(
            result,
            Err(DownloadError::InvalidRequest("parte sin verificar"))
        ));
        for file in &files {
            let expected = file.dest.file_name().expect("nombre").as_encoded_bytes();
            assert_eq!(
                std::fs::read(part_path(&file.dest)).expect("leer"),
                expected
            );
            assert!(!file.dest.exists());
        }
    }

    /// Simula la caída a mitad de `finalize_unit`: la parte ya renombrada.
    fn rename_to_final(file: &FileSpec) {
        std::fs::rename(part_path(&file.dest), &file.dest).expect("renombrar");
    }

    #[test]
    fn finalize_reanuda_tras_caida_a_mitad() {
        let dir = tempfile::tempdir().expect("tmp");
        let files = verified_unit(&dir, &["a.gguf", "b.gguf"]);
        rename_to_final(&files[0]);

        finalize_unit(&files).expect("finalizar");

        for file in &files {
            let expected = file.dest.file_name().expect("nombre").as_encoded_bytes();
            assert_eq!(std::fs::read(&file.dest).expect("final"), expected);
            assert!(!part_path(&file.dest).exists());
        }
        finalize_unit(&files).expect("segunda llamada");
    }

    #[test]
    fn finalize_final_de_otro_tamano_no_cuenta_como_renombrada() {
        let dir = tempfile::tempdir().expect("tmp");
        let files = verified_unit(&dir, &["a.gguf"]);
        std::fs::remove_file(part_path(&files[0].dest)).expect("quitar .part");
        std::fs::write(&files[0].dest, [0u8; 3]).expect("destino de otro tamaño");

        let result = finalize_unit(&files);

        assert!(matches!(result, Err(DownloadError::Io(_))), "{result:?}");
        assert_eq!(std::fs::read(&files[0].dest).expect("leer"), [0u8; 3]);
    }

    #[test]
    fn finalize_revierte_tambien_las_ya_renombradas() {
        let dir = tempfile::tempdir().expect("tmp");
        let files = verified_unit(&dir, &["a.gguf", "b.gguf", "c.gguf"]);
        rename_to_final(&files[0]);
        std::fs::remove_file(part_path(&files[2].dest)).expect("quitar .part");

        let result = finalize_unit(&files);

        assert!(matches!(result, Err(DownloadError::Io(_))), "{result:?}");
        for file in &files {
            assert!(!file.dest.exists());
        }
        for file in &files[..2] {
            let expected = file.dest.file_name().expect("nombre").as_encoded_bytes();
            assert_eq!(
                std::fs::read(part_path(&file.dest)).expect("leer"),
                expected
            );
        }
    }

    #[test]
    fn renombrada_exige_verified_sin_part_y_final_del_tamano() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut files = verified_unit(&dir, &["a.gguf"]);
        assert!(!is_already_renamed(&files[0]));

        rename_to_final(&files[0]);
        assert!(is_already_renamed(&files[0]));

        files[0].expected_size += 1;
        assert!(!is_already_renamed(&files[0]));
        files[0].expected_size -= 1;
        files[0].status = FileStatus::Pending;
        assert!(!is_already_renamed(&files[0]));
    }

    #[test]
    fn perdida_es_verified_sin_part_ni_final_del_tamano() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut files = verified_unit(&dir, &["a.gguf"]);
        assert!(!is_lost(&files[0]));

        std::fs::remove_file(part_path(&files[0].dest)).expect("quitar .part");
        assert!(is_lost(&files[0]));

        std::fs::write(&files[0].dest, [0u8; 3]).expect("final de otro tamaño");
        assert!(is_lost(&files[0]));

        files[0].status = FileStatus::Pending;
        assert!(!is_lost(&files[0]));
    }

    #[test]
    fn finalize_ignora_los_preexistentes() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut files = verified_unit(&dir, &["a.gguf"]);
        files[0].status = FileStatus::Preexisting;
        std::fs::write(&files[0].dest, b"previo").expect("destino previo");

        finalize_unit(&files).expect("finalizar");

        assert_eq!(std::fs::read(&files[0].dest).expect("leer"), b"previo");
    }
}

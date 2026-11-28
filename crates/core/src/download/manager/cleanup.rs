//! Borrado de los ficheros de una unidad al cancelarla o quitarla, sin tocar los
//! que otra unidad de la cola sigue necesitando (Diseño §8, #72).
use std::collections::HashSet;
use std::path::PathBuf;

use super::dest_key;
use crate::download::error::DownloadError;
use crate::download::model::{DownloadId, DownloadUnit, FileStatus, UnitState};
use crate::download::paths::part_path;
use crate::download::transfer::remove_if_exists;
use crate::download::verify::is_already_renamed;

/// Destinos (como `dest_key`) que protegen las demás unidades.
#[derive(Debug, Default)]
pub(super) struct ProtectedKeys {
    /// Destinos cuyo `.part` sigue en uso.
    parts: HashSet<String>,
    /// Destinos cuyo final da por suyo otra unidad.
    finals: HashSet<String>,
}

/// Qué protegen las unidades de `units` distintas de `except`. Un `.part` lo
/// protege toda unidad que aún puede ejecutarse (no terminal o `Failed`, el
/// mismo criterio que `AlreadyQueued`). Un final lo protege toda unidad no
/// `Cancelled` que lo tenga `Preexisting` o ya renombrado.
pub(super) fn protected_keys(units: &[DownloadUnit], except: DownloadId) -> ProtectedKeys {
    let mut protected = ProtectedKeys::default();
    for unit in units.iter().filter(|unit| unit.id != except) {
        for file in &unit.files {
            let key = dest_key(&file.dest);
            if unit.state.blocks_destination() {
                protected.parts.insert(key.clone());
            }
            let owns_final = file.status == FileStatus::Preexisting || is_already_renamed(file);
            if unit.state != UnitState::Cancelled && owns_final {
                protected.finals.insert(key);
            }
        }
    }
    protected
}

/// Qué ficheros de la unidad se borran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    /// Los `.part` y los finales de las partes ya renombradas.
    Everything,
    /// Solo los `.part`: el final puede pertenecer ya a otra descarga.
    PartsOnly,
}

/// Rutas que borra la unidad según `scope`, salvo lo protegido. Se decide todo
/// antes de borrar nada porque borrar un `.part` cambia el resultado de
/// `is_already_renamed`.
fn doomed_paths(unit: &DownloadUnit, units: &[DownloadUnit], scope: Scope) -> Vec<PathBuf> {
    let protected = protected_keys(units, unit.id);
    unit.files
        .iter()
        .flat_map(|file| {
            let key = dest_key(&file.dest);
            let part = (!protected.parts.contains(&key)).then(|| part_path(&file.dest));
            let renamed = (scope == Scope::Everything
                && is_already_renamed(file)
                && !protected.finals.contains(&key))
            .then(|| file.dest.clone());
            part.into_iter().chain(renamed)
        })
        .collect()
}

/// Borra lo que es de `unit` según `scope` y no protege otra unidad de `units`.
/// Intenta borrarlo todo aunque algo falle y devuelve el primer error.
pub(super) async fn delete_unit_files(
    unit: &DownloadUnit,
    units: &[DownloadUnit],
    scope: Scope,
) -> Result<(), DownloadError> {
    let mut first_error = None;
    for path in doomed_paths(unit, units, scope) {
        if let Err(error) = remove_if_exists(&path).await {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::download::manager::dest_key;
    use crate::download::test_harness::unit_of;
    use crate::download::test_support::file_spec;
    use crate::download::{DownloadId, FailureKind, FileSpec, FileStatus, UnitState};

    const SIZE: usize = 4;

    /// Parte de `dir/name`; `on_disk` crea el final y `part` crea el `.part`.
    fn file(dir: &Path, name: &str, status: FileStatus, on_disk: bool, part: bool) -> FileSpec {
        let dest = dir.join(name);
        if on_disk {
            fs::write(&dest, [1u8; SIZE]).expect("final");
        }
        if part {
            fs::write(crate::download::part_path(&dest), [1u8; SIZE]).expect(".part");
        }
        FileSpec {
            status,
            ..file_spec(String::new(), &dest, SIZE)
        }
    }

    fn keys(dir: &Path, names: &[&str]) -> HashSet<String> {
        names.iter().map(|name| dest_key(&dir.join(name))).collect()
    }

    #[test]
    fn protected_keys_segun_estado() {
        let dir = tempfile::tempdir().expect("tmp");
        let at = dir.path();
        let pending = |name: &str| file(at, name, FileStatus::Pending, false, false);
        let renamed = |name: &str| file(at, name, FileStatus::Verified, true, false);
        let units = vec![
            unit_of(1, UnitState::Paused, vec![pending("i")]),
            unit_of(2, UnitState::Queued, vec![pending("a")]),
            unit_of(3, UnitState::Paused, vec![pending("b")]),
            unit_of(
                4,
                UnitState::Failed(FailureKind::Network),
                vec![pending("c"), renamed("j")],
            ),
            unit_of(5, UnitState::Completed, vec![renamed("d")]),
            unit_of(
                6,
                UnitState::Completed,
                vec![file(at, "e", FileStatus::Verified, false, false)],
            ),
            unit_of(
                7,
                UnitState::Cancelled,
                vec![
                    file(at, "f", FileStatus::Verified, true, false),
                    file(at, "g", FileStatus::Preexisting, true, false),
                ],
            ),
            unit_of(
                8,
                UnitState::Completed,
                vec![file(at, "h", FileStatus::Preexisting, true, false)],
            ),
        ];

        let protected = protected_keys(&units, DownloadId(1));

        assert_eq!(protected.parts, keys(at, &["a", "b", "c", "j"]));
        assert_eq!(protected.finals, keys(at, &["d", "h", "j"]));
    }
}

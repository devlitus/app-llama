//! Ejecución de una unidad de descarga: espacio, partes, verificación y cierre.
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio_util::sync::CancellationToken;

use super::auth::auth_source;
use super::disk::{check_space, required_bytes};
use super::error::DownloadError;
use super::events::{DownloadProgress, EventSink, ProgressPhase};
use super::manager::DownloadManager;
use super::model::{DownloadId, DownloadUnit, FailureKind, FileSpec, FileStatus, UnitState};
use super::paths::part_path;
use super::progress::{Progress, ProgressReporter};
use super::retry::{RetryPolicy, TransferObserver, transfer_with_retry};
use super::transfer::{TransferOutcome, part_len, remove_if_exists};
use super::verify::{UNVERIFIED_PART, finalize_unit, is_lost, verify_file};

/// Ejecuta la unidad hasta dejarla en un estado final y libera su hueco en la
/// cola. Si la tarea se cancela, el estado lo decide quien la canceló.
pub(super) async fn run_unit(
    manager: Arc<DownloadManager>,
    id: DownloadId,
    cancel: CancellationToken,
) {
    match execute(&manager, id, &cancel).await {
        Ok(()) | Err(DownloadError::Cancelled) => {}
        Err(error) => {
            // Si ni siquiera se puede registrar el fallo, la cola ya lo tiene en memoria.
            let _ = manager
                .transition(id, UnitState::Failed(unit_failure(error)))
                .await;
        }
    }
    manager.finish_run(id).await;
}

/// Orden de una unidad (Diseño §7 y §9): devolver a `Pending` las partes
/// perdidas, clasificar las candidatas a `Preexisting`, comprobar el espacio,
/// descargar y verificar cada parte, verificar las candidatas ya en `Verifying`
/// y finalizar.
async fn execute(
    manager: &DownloadManager,
    id: DownloadId,
    cancel: &CancellationToken,
) -> Result<(), DownloadError> {
    let unit = manager.unit(id).await?;
    let mut run = UnitRun::new(manager, unit, cancel).await?;
    run.ensure_space()?;
    manager.transition(id, UnitState::Downloading).await?;
    run.download_pending().await?;
    manager.transition(id, UnitState::Verifying).await?;
    run.verify_candidates().await?;
    finalize_unit(&run.files)?;
    manager.transition(id, UnitState::Completed).await
}

/// Una parte `Pending` cuyo destino final ya existe con el tamaño esperado.
/// Solo mira `metadata`: el hash se calcula después, en `Verifying`.
fn is_preexisting_candidate(file: &FileSpec) -> bool {
    file.status == FileStatus::Pending
        && std::fs::metadata(&file.dest).is_ok_and(|metadata| metadata.len() == file.expected_size)
}

struct UnitRun<'a> {
    manager: &'a DownloadManager,
    cancel: &'a CancellationToken,
    id: DownloadId,
    files: Vec<FileSpec>,
    candidates: Vec<bool>,
    /// Bytes de cada fichero que ya estaban disponibles al empezar.
    initial: Vec<u64>,
    progress: ProgressEmitter,
}

impl<'a> UnitRun<'a> {
    async fn new(
        manager: &'a DownloadManager,
        unit: DownloadUnit,
        cancel: &'a CancellationToken,
    ) -> Result<Self, DownloadError> {
        let id = unit.id;
        let mut files = unit.files;
        reset_lost_parts(manager, id, &mut files).await;
        let candidates: Vec<bool> = files.iter().map(is_preexisting_candidate).collect();
        let mut initial = Vec::with_capacity(files.len());
        for (file, candidate) in files.iter().zip(&candidates) {
            initial.push(available_bytes(file, *candidate).await?);
        }
        let progress = ProgressEmitter::new(manager, id, &files, &candidates, &initial);
        Ok(Self {
            manager,
            cancel,
            id,
            files,
            candidates,
            initial,
            progress,
        })
    }

    fn needs_download(&self, index: usize) -> bool {
        self.files[index].status == FileStatus::Pending && !self.candidates[index]
    }

    /// Solo se pide espacio para lo que se va a escribir: las candidatas nunca
    /// se descargan y, sin nada que escribir, no se exige ni el margen.
    fn ensure_space(&self) -> Result<(), DownloadError> {
        let to_download: Vec<(FileSpec, u64)> = (0..self.files.len())
            .filter(|index| self.needs_download(*index))
            .map(|index| (self.files[index].clone(), self.initial[index]))
            .collect();
        let (files, on_disk): (Vec<FileSpec>, Vec<u64>) = to_download.into_iter().unzip();
        let required = required_bytes(&files, &on_disk);
        match files.first() {
            Some(first) if required > 0 => {
                check_space(self.manager.deps.disk.as_ref(), &first.dest, required)
            }
            _ => Ok(()),
        }
    }

    async fn download_pending(&mut self) -> Result<(), DownloadError> {
        let pending: Vec<usize> = (0..self.files.len())
            .filter(|index| self.needs_download(*index))
            .collect();
        for index in &pending {
            self.download_part(*index).await?;
        }
        if !pending.is_empty() {
            self.progress.finish_download();
        }
        Ok(())
    }

    async fn download_part(&mut self, index: usize) -> Result<(), DownloadError> {
        let spec = self.files[index].clone();
        match self.transfer(&spec, self.rewind_base(index)).await? {
            TransferOutcome::Completed => {}
            TransferOutcome::Cancelled => return Err(DownloadError::Cancelled),
            // `transfer_with_retry` solo devuelve estos dos al agotar los intentos.
            TransferOutcome::Interrupted { .. } | TransferOutcome::Restart => {
                return Err(DownloadError::Failed(FailureKind::Network));
            }
        }
        self.verify_downloaded(&spec).await?;
        self.settle(index, FileStatus::Verified).await;
        Ok(())
    }

    /// Bytes de la unidad que siguen valiendo si la parte `current` vuelve a
    /// empezar de cero: las ya asentadas y lo que traían las demás.
    fn rewind_base(&self, current: usize) -> u64 {
        self.files
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != current)
            .map(|(index, file)| {
                if file.status == FileStatus::Pending {
                    self.initial[index]
                } else {
                    file.expected_size
                }
            })
            .sum()
    }

    async fn transfer(
        &self,
        spec: &FileSpec,
        rewind_base: u64,
    ) -> Result<TransferOutcome, DownloadError> {
        let deps = &self.manager.deps;
        let auth = auth_source(&spec.url, &deps.hf_base_url, deps.tokens.as_ref());
        let progress = &self.progress;
        transfer_with_retry(
            &deps.fetcher,
            spec,
            &auth,
            &RetryPolicy::default(),
            deps.sleeper.as_ref(),
            self.cancel,
            TransferObserver {
                on_bytes: &mut |bytes| progress.downloaded(bytes),
                on_restart: &mut || progress.rewind(rewind_base),
            },
        )
        .await
    }

    /// Un tamaño o un hash incorrectos invalidan el `.part`: se borra.
    async fn verify_downloaded(&self, spec: &FileSpec) -> Result<(), DownloadError> {
        let part = part_path(&spec.dest);
        let result = self.verify(&part, spec).await;
        if matches!(
            result,
            Err(DownloadError::Failed(
                FailureKind::SizeMismatch { .. } | FailureKind::ChecksumMismatch
            ))
        ) {
            remove_if_exists(&part).await?;
        }
        result
    }

    /// Verifica las candidatas sobre su final, sin tocarlo. Si no coinciden, el
    /// fichero es del usuario: no se borra y la unidad falla.
    async fn verify_candidates(&mut self) -> Result<(), DownloadError> {
        for index in 0..self.files.len() {
            if !self.candidates[index] {
                continue;
            }
            let spec = self.files[index].clone();
            match self.verify(&spec.dest, &spec).await {
                Ok(()) => self.settle(index, FileStatus::Preexisting).await,
                Err(DownloadError::Failed(
                    FailureKind::SizeMismatch { .. } | FailureKind::ChecksumMismatch,
                )) => return Err(DownloadError::DestinationExists),
                Err(error) => return Err(error),
            }
        }
        self.progress.finish_verify();
        Ok(())
    }

    async fn verify(&self, path: &std::path::Path, spec: &FileSpec) -> Result<(), DownloadError> {
        let progress = self.progress.clone();
        verify_file(
            path,
            spec.expected_size,
            spec.sha256.as_deref(),
            self.cancel,
            move |bytes| progress.verified(bytes),
        )
        .await
    }

    async fn settle(&mut self, index: usize, status: FileStatus) {
        self.files[index].status = status;
        self.manager.mark_file(self.id, index, status).await;
    }
}

/// Paso (0) de la ejecución (Diseño §7): las partes `Verified` de las que no
/// queda ni el `.part` ni el final, porque se borraron mientras la unidad estaba
/// en pausa, vuelven a `Pending`. Va antes de clasificar las candidatas y de
/// comprobar el espacio, para que se descarguen y cuenten como cualquier otra.
async fn reset_lost_parts(manager: &DownloadManager, id: DownloadId, files: &mut [FileSpec]) {
    for (index, file) in files.iter_mut().enumerate() {
        if is_lost(file) {
            file.status = FileStatus::Pending;
            manager.mark_file(id, index, FileStatus::Pending).await;
        }
    }
}

/// Bytes que una parte ya tiene al empezar: su final si es candidata, su
/// `.part` si está pendiente y todo su tamaño si ya está verificada.
async fn available_bytes(file: &FileSpec, candidate: bool) -> Result<u64, DownloadError> {
    if file.status != FileStatus::Pending || candidate {
        return Ok(file.expected_size);
    }
    Ok(part_len(&part_path(&file.dest))
        .await?
        .min(file.expected_size))
}

/// Progreso de una ejecución. Se comparte (`Arc`) porque el hash corre en un
/// hilo de bloqueo y su callback no puede tomar nada prestado.
///
/// Contrato: un solo reporter (un solo limitador) con el `total` de la unidad,
/// constante en las dos fases. En `Download`, `downloaded_bytes` son los bytes
/// recibidos. En `Verify`, son los bytes que no requieren hash (sin SHA256 o ya
/// verificados), contados como hechos desde el principio, más los hasheados.
#[derive(Clone)]
struct ProgressEmitter {
    id: DownloadId,
    sink: Arc<dyn EventSink>,
    state: Arc<Mutex<EmitterState>>,
}

struct EmitterState {
    reporter: ProgressReporter,
    downloaded: u64,
    unhashed: u64,
    verified: u64,
}

impl EmitterState {
    fn verify_position(&self) -> u64 {
        self.unhashed + self.verified
    }
}

impl ProgressEmitter {
    fn new(
        manager: &DownloadManager,
        id: DownloadId,
        files: &[FileSpec],
        candidates: &[bool],
        initial: &[u64],
    ) -> Self {
        let deps = &manager.deps;
        let total: u64 = files.iter().map(|file| file.expected_size).sum();
        let hashed: u64 = files
            .iter()
            .zip(candidates)
            .filter(|(file, candidate)| {
                file.sha256.is_some() && (file.status == FileStatus::Pending || **candidate)
            })
            .map(|(file, _)| file.expected_size)
            .sum();
        let downloaded = initial.iter().sum();
        let state = EmitterState {
            reporter: ProgressReporter::new(total, downloaded, deps.clock.clone()),
            downloaded,
            unhashed: total - hashed,
            verified: 0,
        };
        Self {
            id,
            sink: Arc::clone(&deps.sink),
            state: Arc::new(Mutex::new(state)),
        }
    }

    fn downloaded(&self, bytes: u64) {
        let progress = {
            let mut state = lock(&self.state);
            state.downloaded = state.downloaded.saturating_add(bytes);
            let downloaded = state.downloaded;
            state.reporter.record_at(bytes, downloaded)
        };
        if let Some(progress) = progress {
            self.emit(&progress, ProgressPhase::Download);
        }
    }

    /// La parte vuelve a empezar de cero: la barra baja al momento a `base`.
    fn rewind(&self, base: u64) {
        let progress = {
            let mut state = lock(&self.state);
            state.downloaded = base;
            state.reporter.rewind(base)
        };
        self.emit(&progress, ProgressPhase::Download);
    }

    fn finish_download(&self) {
        let progress = {
            let mut state = lock(&self.state);
            let downloaded = state.downloaded;
            state.reporter.finish_at(downloaded)
        };
        self.emit(&progress, ProgressPhase::Download);
    }

    fn verified(&self, bytes: u64) {
        let progress = {
            let mut state = lock(&self.state);
            state.verified = state.verified.saturating_add(bytes);
            let position = state.verify_position();
            state.reporter.record_at(bytes, position)
        };
        if let Some(progress) = progress {
            self.emit(&progress, ProgressPhase::Verify);
        }
    }

    fn finish_verify(&self) {
        let progress = {
            let mut state = lock(&self.state);
            let position = state.verify_position();
            state.reporter.finish_at(position)
        };
        if progress.total > 0 {
            self.emit(&progress, ProgressPhase::Verify);
        }
    }

    fn emit(&self, progress: &Progress, phase: ProgressPhase) {
        self.sink
            .progress(&DownloadProgress::new(self.id, progress, phase));
    }
}

/// Un hilo que entró en pánico con el cerrojo no invalida los contadores.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

const INTERNAL_ERROR: &str = "error interno inesperado de la descarga";

/// Traduce el error de un paso de la ejecución al motivo que ve el usuario.
/// Los mensajes son fijos: un `io::Error` o un `InvalidRequest` pueden arrastrar
/// rutas locales. Sin brazo `_`: una variante nueva obliga a decidir su traducción.
pub(super) fn unit_failure(error: DownloadError) -> FailureKind {
    match error {
        DownloadError::Failed(kind) => kind,
        DownloadError::DestinationExists => FailureKind::DestinationExists,
        DownloadError::InvalidRequest(UNVERIFIED_PART) => {
            io_failure("parte sin verificar al finalizar la unidad")
        }
        DownloadError::InvalidRequest(reason) => {
            io_failure(&format!("petición no válida: {reason}"))
        }
        DownloadError::Io(_) => io_failure("error de lectura o escritura en el disco"),
        // Inalcanzables en la ejecución: acabar en una unidad fallida visible es
        // mejor que tumbar la tarea.
        DownloadError::UnsafePath
        | DownloadError::IncompleteSplit
        | DownloadError::AlreadyQueued
        | DownloadError::InvalidTransition
        | DownloadError::UnknownDownload
        | DownloadError::Cancelled => io_failure(INTERNAL_ERROR),
    }
}

fn io_failure(message: &str) -> FailureKind {
    FailureKind::Io {
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use wiremock::MockServer;

    use super::*;
    use crate::download::events::{DownloadProgress, ProgressPhase};
    use crate::download::paths::part_path;
    use crate::download::test_harness::{
        GIB, Harness, HookFactory, KIB, MIB, Options, SinkEvent, part, part_without_sha,
        request_count, serve, sha_hex,
    };
    use crate::download::test_support::{cutting_server, pattern_bytes};
    use crate::download::{Clock, DownloadId, FileStatus, PartRequest, SteppingClock, UnitState};

    fn count_gguf(dir: &Path) -> usize {
        fs::read_dir(dir).map_or(0, |entries| {
            entries
                .flatten()
                .map(|entry| {
                    let path = entry.path();
                    if path.is_dir() {
                        count_gguf(&path)
                    } else {
                        usize::from(path.extension().is_some_and(|ext| ext == "gguf"))
                    }
                })
                .sum()
        })
    }

    #[tokio::test]
    async fn unidad_simple_completa() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        serve(&server, "m.gguf", &body).await;
        let h = Harness::new(&server.uri());

        let id = h.enqueue(vec![part("m.gguf", &body)]).await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        assert_eq!(
            h.states(id),
            [
                UnitState::Queued,
                UnitState::Downloading,
                UnitState::Verifying,
                UnitState::Completed
            ]
        );
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
        assert!(!part_path(&h.dest("m.gguf")).exists());
    }

    #[tokio::test]
    async fn progreso_limitado_en_frecuencia() {
        let body = pattern_bytes(10 * MIB);
        let server = MockServer::start().await;
        serve(&server, "m.gguf", &body).await;
        let clock = Arc::new(SteppingClock::new(Duration::from_millis(50)));
        let h = Harness::configured(
            &server.uri(),
            Options {
                clock: clock.clone(),
                ..Options::default()
            },
        );

        let id = h
            .enqueue(vec![part_without_sha("m.gguf", body.len() as u64)])
            .await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        let t_final_ms = clock.now().as_millis();
        let events = h.sink.progress();
        let bound = usize::try_from(t_final_ms.div_ceil(250)).expect("cota") + 2;
        assert!(
            events.len() <= bound,
            "{} eventos, cota {bound}",
            events.len()
        );
        let last = events.last().expect("hay progreso");
        assert_eq!(last.downloaded_bytes, last.total_bytes);
        assert_eq!(last.total_bytes, body.len() as u64);
    }

    #[tokio::test]
    async fn dividido_agrega_progreso() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        let names = [
            "m-00001-of-00003.gguf",
            "m-00002-of-00003.gguf",
            "m-00003-of-00003.gguf",
        ];
        for name in names {
            serve(&server, name, &body).await;
        }
        let early_finals = Arc::new(AtomicBool::new(false));
        let verifications = Arc::new(AtomicUsize::new(0));
        let (flag, seen) = (early_finals.clone(), verifications.clone());
        let hook: HookFactory = Box::new(move |models_dir| {
            let models_dir = models_dir.to_path_buf();
            Arc::new(move |event| {
                let verifying = match event {
                    SinkEvent::State(snapshot) => snapshot.state == UnitState::Verifying,
                    SinkEvent::Progress(progress) => progress.phase == ProgressPhase::Verify,
                };
                if verifying {
                    seen.fetch_add(1, Ordering::SeqCst);
                    if count_gguf(&models_dir) > 0 {
                        flag.store(true, Ordering::SeqCst);
                    }
                }
            })
        });
        let h = Harness::configured(
            &server.uri(),
            Options {
                hook: Some(hook),
                ..Options::default()
            },
        );

        let id = h
            .enqueue(names.iter().map(|name| part(name, &body)).collect())
            .await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        assert!(verifications.load(Ordering::SeqCst) > 0);
        assert!(!early_finals.load(Ordering::SeqCst));
        let events = h.sink.progress();
        assert_eq!(events.last().expect("progreso").total_bytes, 3_145_728);
        let download_end = events
            .iter()
            .rfind(|event| event.phase == ProgressPhase::Download)
            .expect("progreso de descarga");
        assert_eq!(download_end.downloaded_bytes, 3_145_728);
        assert_eq!(count_gguf(h.dir.path()), 3);
    }

    #[tokio::test]
    async fn fifo_y_un_activo() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        for name in ["a.gguf", "b.gguf", "c.gguf"] {
            serve(&server, name, &body).await;
        }
        let h = Harness::new(&server.uri());

        let mut ids = Vec::new();
        for name in ["a.gguf", "b.gguf", "c.gguf"] {
            ids.push(h.enqueue(vec![part(name, &body)]).await);
        }

        assert_eq!(h.finished(ids[2]).await, UnitState::Completed);
        let mut active: Vec<DownloadId> = Vec::new();
        let mut started = Vec::new();
        for snapshot in h.sink.states() {
            match snapshot.state {
                UnitState::Downloading => {
                    active.push(snapshot.id);
                    started.push(snapshot.id);
                }
                UnitState::Completed | UnitState::Failed(_) => {
                    active.retain(|id| *id != snapshot.id);
                }
                _ => {}
            }
            assert!(active.len() <= 1, "dos unidades activas: {active:?}");
        }
        assert_eq!(started, ids);
    }

    #[tokio::test]
    async fn sin_espacio_no_pide_nada() {
        let server = MockServer::start().await;
        let h = Harness::configured(
            &server.uri(),
            Options {
                disk: 40 * GIB,
                ..Options::default()
            },
        );

        let id = h.enqueue(vec![part_without_sha("m.gguf", 60 * GIB)]).await;

        let expected = UnitState::Failed(FailureKind::InsufficientSpace {
            needed: 65_498_251_264,
            available: 42_949_672_960,
        });
        assert_eq!(h.finished(id).await, expected);
        assert_eq!(h.states(id), [UnitState::Queued, expected]);
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn checksum_malo_borra_part() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        serve(&server, "m.gguf", &body).await;
        let h = Harness::new(&server.uri());
        let wrong = PartRequest {
            sha256: Some(sha_hex(b"otro contenido")),
            ..part("m.gguf", &body)
        };

        let id = h.enqueue(vec![wrong]).await;

        assert_eq!(
            h.finished(id).await,
            UnitState::Failed(FailureKind::ChecksumMismatch)
        );
        assert!(!part_path(&h.dest("m.gguf")).exists());
        assert!(!h.dest("m.gguf").exists());
    }

    #[tokio::test]
    async fn tamano_malo_borra_part() {
        let server = MockServer::start().await;
        serve(&server, "m.gguf", &pattern_bytes(MIB + 1)).await;
        let h = Harness::new(&server.uri());

        let id = h
            .enqueue(vec![part_without_sha("m.gguf", MIB as u64)])
            .await;

        let state = h.finished(id).await;
        assert!(
            matches!(
                state,
                UnitState::Failed(FailureKind::SizeMismatch { expected, .. })
                    if expected == MIB as u64
            ),
            "{state:?}"
        );
        assert!(!part_path(&h.dest("m.gguf")).exists());
    }

    #[tokio::test]
    async fn tamano_corto_borra_part() {
        let server = MockServer::start().await;
        serve(&server, "m.gguf", &pattern_bytes(MIB - 1)).await;
        let h = Harness::new(&server.uri());

        let id = h
            .enqueue(vec![part_without_sha("m.gguf", MIB as u64)])
            .await;

        assert_eq!(
            h.finished(id).await,
            UnitState::Failed(FailureKind::SizeMismatch {
                expected: MIB as u64,
                actual: MIB as u64 - 1
            })
        );
        assert_eq!(request_count(&server).await, 1);
        assert!(!part_path(&h.dest("m.gguf")).exists());
        assert!(!h.dest("m.gguf").exists());
    }

    #[tokio::test]
    async fn preexistente_no_se_descarga() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        let h = Harness::new(&server.uri());
        h.write_final("m.gguf", &body);

        let id = h.enqueue(vec![part("m.gguf", &body)]).await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        assert_eq!(h.file_status(id, 0).await, FileStatus::Preexisting);
        assert_eq!(request_count(&server).await, 0);
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
    }

    #[tokio::test]
    async fn preexistente_hash_distinto_falla() {
        let body = pattern_bytes(MIB);
        let other = vec![7u8; MIB];
        let server = MockServer::start().await;
        let h = Harness::new(&server.uri());
        h.write_final("m.gguf", &other);

        let id = h.enqueue(vec![part("m.gguf", &body)]).await;

        assert_eq!(
            h.finished(id).await,
            UnitState::Failed(FailureKind::DestinationExists)
        );
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), other);
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn reencolar_completado_sin_espacio_completa_sin_peticiones() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        let h = Harness::configured(
            &server.uri(),
            Options {
                disk: 0,
                ..Options::default()
            },
        );
        h.write_final("m.gguf", &body);

        let id = h.enqueue(vec![part("m.gguf", &body)]).await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        assert_eq!(h.file_status(id, 0).await, FileStatus::Preexisting);
        assert_eq!(request_count(&server).await, 0);
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
    }

    fn with_disk(disk: u64) -> Options {
        Options {
            disk,
            ..Options::default()
        }
    }

    const SPLIT: [&str; 2] = ["m-00001-of-00002.gguf", "m-00002-of-00002.gguf"];

    /// Unidad de dos partes de 1 MiB con la primera ya en disco como candidata.
    async fn split_with_preexisting_first(options: Options) -> (MockServer, Harness, DownloadId) {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        serve(&server, SPLIT[1], &body).await;
        let h = Harness::configured(&server.uri(), options);
        h.write_final(SPLIT[0], &body);
        let id = h
            .enqueue(SPLIT.iter().map(|name| part(name, &body)).collect())
            .await;
        (server, h, id)
    }

    #[tokio::test]
    async fn candidata_no_cuenta_para_el_espacio() {
        let (server, h, id) = split_with_preexisting_first(with_disk(MIB as u64 + GIB)).await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        assert_eq!(request_count(&server).await, 1);
        assert_eq!(h.file_status(id, 0).await, FileStatus::Preexisting);
    }

    #[tokio::test]
    async fn candidata_sin_espacio_para_el_resto_falla() {
        let (server, h, id) = split_with_preexisting_first(with_disk(MIB as u64 + GIB - 1)).await;

        let expected = UnitState::Failed(FailureKind::InsufficientSpace {
            needed: 1_074_790_400,
            available: 1_074_790_399,
        });
        assert_eq!(h.finished(id).await, expected);
        assert_eq!(h.states(id), [UnitState::Queued, expected]);
        assert_eq!(request_count(&server).await, 0);
        assert_eq!(
            fs::read(h.dest(SPLIT[0])).expect("final"),
            pattern_bytes(MIB)
        );
    }

    #[test]
    fn unit_failure_parte_sin_verificar_es_io() {
        let kind = unit_failure(DownloadError::InvalidRequest("parte sin verificar"));

        assert_eq!(
            kind,
            FailureKind::Io {
                message: "parte sin verificar al finalizar la unidad".to_string()
            }
        );
    }

    #[test]
    fn unit_failure_conserva_failed_y_destino() {
        let failed = unit_failure(DownloadError::Failed(FailureKind::ChecksumMismatch));
        let destination = unit_failure(DownloadError::DestinationExists);

        assert_eq!(failed, FailureKind::ChecksumMismatch);
        assert_eq!(destination, FailureKind::DestinationExists);
    }

    #[test]
    fn unit_failure_io_sin_ruta() {
        let kind = unit_failure(DownloadError::Io(std::io::Error::other(
            "fallo en /secreto/ruta",
        )));

        let FailureKind::Io { message } = kind else {
            panic!("se esperaba Io, llegó {kind:?}");
        };
        assert!(!message.contains("secreto"), "{message}");
    }

    #[test]
    fn unit_failure_otra_peticion_invalida() {
        let kind = unit_failure(DownloadError::InvalidRequest("sha256 no válido"));

        assert_eq!(
            kind,
            FailureKind::Io {
                message: "petición no válida: sha256 no válido".to_string()
            }
        );
    }

    #[test]
    fn unit_failure_variantes_inalcanzables() {
        let unreachable = [
            DownloadError::UnsafePath,
            DownloadError::IncompleteSplit,
            DownloadError::AlreadyQueued,
            DownloadError::InvalidTransition,
            DownloadError::UnknownDownload,
            DownloadError::Cancelled,
        ];
        for error in unreachable {
            let label = format!("{error:?}");
            assert_eq!(
                unit_failure(error),
                FailureKind::Io {
                    message: "error interno inesperado de la descarga".to_string()
                },
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn persiste_en_cambios_de_estado() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        serve(&server, "m.gguf", &body).await;
        let h = Harness::new(&server.uri());

        let id = h.enqueue(vec![part("m.gguf", &body)]).await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        assert_eq!(h.store.save_count(), h.states(id).len());
        let saved = h.store.last_saved().expect("guardado");
        assert_eq!(saved.units[0].state, UnitState::Completed);
    }

    #[tokio::test]
    async fn fallo_red_conserva_part() {
        let body = pattern_bytes(2 * MIB);
        let mut cuts = vec![Some(MIB)];
        cuts.extend(vec![Some(0); 5]);
        let server = cutting_server(body.clone(), cuts, false).await;
        let h = Harness::new(&server.base_url());

        let id = h
            .enqueue(vec![part_without_sha("m.gguf", body.len() as u64)])
            .await;

        assert_eq!(
            h.finished(id).await,
            UnitState::Failed(FailureKind::Network)
        );
        let part_len = fs::metadata(part_path(&h.dest("m.gguf")))
            .expect(".part")
            .len();
        assert_eq!(part_len, MIB as u64);
    }

    #[tokio::test]
    async fn reinicio_no_supera_total() {
        let body = pattern_bytes(256 * KIB);
        let server = cutting_server(body.clone(), Vec::new(), true).await;
        let h = Harness::new(&server.base_url());
        h.write_part("m.gguf", &body[..100 * KIB]);

        let id = h.enqueue(vec![part_without_sha("m.gguf", 262_144)]).await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        let events = h.sink.progress();
        assert!(
            events
                .iter()
                .all(|e| e.downloaded_bytes <= 262_144 && e.total_bytes == 262_144)
        );
        assert!(events.iter().any(|e| e.downloaded_bytes == 0));
        assert_eq!(events.last().expect("progreso").downloaded_bytes, 262_144);
    }

    #[tokio::test]
    async fn reinicio_en_segunda_parte_rebobina_a_base() {
        let body = pattern_bytes(128 * KIB);
        let server = cutting_server(body.clone(), Vec::new(), true).await;
        let h = Harness::new(&server.base_url());
        h.write_part(SPLIT[1], &body[..50 * KIB]);

        let id = h
            .enqueue(
                SPLIT
                    .iter()
                    .map(|name| part_without_sha(name, 131_072))
                    .collect(),
            )
            .await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        let events = h.sink.progress();
        assert!(events.iter().all(|e| e.downloaded_bytes <= 262_144));
        assert!(events.iter().any(|e| e.downloaded_bytes == 131_072));
        assert_eq!(events.last().expect("progreso").downloaded_bytes, 262_144);
    }

    /// Reloj que avanza `step` en cada lectura y recuerda la última.
    struct LoggedClock {
        inner: SteppingClock,
        last: Mutex<Duration>,
    }

    impl LoggedClock {
        fn new(step: Duration) -> Self {
            Self {
                inner: SteppingClock::new(step),
                last: Mutex::new(Duration::ZERO),
            }
        }

        fn last(&self) -> Duration {
            *self.last.lock().expect("reloj")
        }
    }

    impl Clock for LoggedClock {
        fn now(&self) -> Duration {
            let now = self.inner.now();
            *self.last.lock().expect("reloj") = now;
            now
        }
    }

    fn stepping(step_ms: u64) -> Options {
        Options {
            clock: Arc::new(SteppingClock::new(Duration::from_millis(step_ms))),
            ..Options::default()
        }
    }

    fn assert_progress_is_coherent(events: &[DownloadProgress], total: u64) {
        assert!(!events.is_empty());
        for event in events {
            assert_eq!(event.total_bytes, total, "{event:?}");
            assert!(event.downloaded_bytes <= total, "{event:?}");
        }
        for phase in [ProgressPhase::Download, ProgressPhase::Verify] {
            let mut previous = 0;
            for event in events.iter().filter(|event| event.phase == phase) {
                assert!(event.downloaded_bytes >= previous, "{phase:?} retrocede");
                previous = event.downloaded_bytes;
            }
        }
    }

    #[tokio::test]
    async fn progreso_verify_no_retrocede() {
        let (_server, h, id) = split_with_preexisting_first(stepping(50)).await;

        assert_eq!(h.finished(id).await, UnitState::Completed);

        let events = h.sink.progress();
        assert_progress_is_coherent(&events, 2 * MIB as u64);
        let first_verify = events
            .iter()
            .position(|event| event.phase == ProgressPhase::Verify)
            .expect("fase de verificación");
        assert!(
            events[..first_verify]
                .iter()
                .all(|event| event.phase == ProgressPhase::Download)
        );
        let last = events.last().expect("progreso");
        assert_eq!(last.phase, ProgressPhase::Verify);
        assert_eq!(last.downloaded_bytes, 2 * MIB as u64);
    }

    #[tokio::test]
    async fn progreso_verify_cuenta_partes_sin_hash() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        for name in SPLIT {
            serve(&server, name, &body).await;
        }
        let h = Harness::configured(&server.uri(), stepping(50));

        let id = h
            .enqueue(vec![
                part_without_sha(SPLIT[0], MIB as u64),
                part(SPLIT[1], &body),
            ])
            .await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        let events = h.sink.progress();
        assert_progress_is_coherent(&events, 2 * MIB as u64);
        let verify: Vec<_> = events
            .iter()
            .filter(|event| event.phase == ProgressPhase::Verify)
            .collect();
        assert!(!verify.is_empty());
        assert!(
            verify
                .iter()
                .all(|event| event.downloaded_bytes >= MIB as u64)
        );
    }

    #[tokio::test]
    async fn progreso_un_solo_limitador() {
        let clock = Arc::new(LoggedClock::new(Duration::from_millis(50)));
        let times = Arc::new(Mutex::new(Vec::new()));
        let (log, reader) = (times.clone(), clock.clone());
        let hook: HookFactory = Box::new(move |_| {
            Arc::new(move |event| {
                if matches!(event, SinkEvent::Progress(_)) {
                    log.lock().expect("tiempos").push(reader.last());
                }
            })
        });
        let (_server, h, id) = split_with_preexisting_first(Options {
            clock: clock.clone(),
            hook: Some(hook),
            ..Options::default()
        })
        .await;

        assert_eq!(h.finished(id).await, UnitState::Completed);

        let events = h.sink.progress();
        let times = times.lock().expect("tiempos").clone();
        assert_eq!(times.len(), events.len());
        let last_download = events
            .iter()
            .rposition(|event| event.phase == ProgressPhase::Download);
        for index in 1..events.len() - 1 {
            if Some(index) == last_download {
                continue;
            }
            let gap = times[index].saturating_sub(times[index - 1]);
            assert!(gap >= Duration::from_millis(250), "evento {index}: {gap:?}");
        }
    }
}

//! Órdenes del usuario sobre la cola: pausar, reanudar, cancelar, reintentar y quitar.
use std::sync::Arc;

use super::Reservation;
use super::cleanup::delete_unit_files;
use crate::download::error::DownloadError;
use crate::download::model::{DownloadId, DownloadUnit, UnitState};

impl super::DownloadManager {
    /// Cancela la transferencia y conserva los `.part`. Espera a que la tarea termine.
    pub async fn pause(&self, id: DownloadId) -> Result<(), DownloadError> {
        let _order = self.orders.lock().await;
        self.unit_for(id, &UnitState::Paused).await?;
        let _reservation = self.stop_run(id).await;
        self.transition(id, UnitState::Paused).await
    }

    pub async fn resume(self: &Arc<Self>, id: DownloadId) -> Result<(), DownloadError> {
        self.requeue(id, |state| matches!(state, UnitState::Paused))
            .await
    }

    /// Vuelve a encolar una unidad fallida; sus `.part` se conservan.
    pub async fn retry(self: &Arc<Self>, id: DownloadId) -> Result<(), DownloadError> {
        self.requeue(id, |state| matches!(state, UnitState::Failed(_)))
            .await
    }

    /// Aborta la unidad y borra sus `.part` y los finales que creó, salvo los
    /// `Preexisting` y los que protege otra unidad. Espera a que la tarea termine.
    pub async fn cancel(&self, id: DownloadId) -> Result<(), DownloadError> {
        let _order = self.orders.lock().await;
        self.unit_for(id, &UnitState::Cancelled).await?;
        let _reservation = self.stop_run(id).await;
        // La tarea pudo acabar en `Completed` o `Failed` mientras se la esperaba (#76):
        // se vuelve a validar antes de borrar nada.
        let unit = self.unit_for(id, &UnitState::Cancelled).await?;
        let cleaned = delete_unit_files(&unit, &self.all_units().await).await;
        self.transition(id, UnitState::Cancelled).await?;
        cleaned
    }

    /// Saca de la lista una unidad terminada. Una `Failed` borra además lo suyo,
    /// como `cancel`; una `Completed` o `Cancelled` no borra nada.
    pub async fn remove(&self, id: DownloadId) -> Result<(), DownloadError> {
        let _order = self.orders.lock().await;
        let unit = self.unit(id).await?;
        if !unit.state.can_remove() {
            return Err(DownloadError::InvalidTransition);
        }
        if matches!(unit.state, UnitState::Failed(_)) {
            delete_unit_files(&unit, &self.all_units().await).await?;
        }
        self.drop_unit(id).await
    }

    /// Pasa la unidad a `Queued` si `from` acepta su estado y arranca lo que pueda.
    async fn requeue(
        self: &Arc<Self>,
        id: DownloadId,
        from: fn(&UnitState) -> bool,
    ) -> Result<(), DownloadError> {
        let _order = self.orders.lock().await;
        let mut queue = self.queue.lock().await;
        if !from(&queue.unit_mut(id)?.state) {
            return Err(DownloadError::InvalidTransition);
        }
        let queued = self.transition_locked(&mut queue, id, UnitState::Queued);
        self.start_waiting(&mut queue);
        queued
    }

    /// La unidad, si puede pasar a `to`.
    async fn unit_for(
        &self,
        id: DownloadId,
        to: &UnitState,
    ) -> Result<DownloadUnit, DownloadError> {
        let unit = self.unit(id).await?;
        if unit.state.can_transition(to) {
            Ok(unit)
        } else {
            Err(DownloadError::InvalidTransition)
        }
    }

    async fn all_units(&self) -> Vec<DownloadUnit> {
        self.queue.lock().await.units.clone()
    }

    /// Cancela la ejecución de `id`, si la tiene, y espera a que su tarea termine.
    /// Siempre con el token, nunca con `abort`: el hilo de hash y los ficheros
    /// abiertos deben liberarse antes de borrar nada.
    ///
    /// Devuelve la reserva de la unidad, que la orden debe conservar hasta
    /// terminar: se toma bajo el mismo cerrojo de la cola que la búsqueda de la
    /// ejecución, para que `start_waiting` no relance la unidad en el hueco entre
    /// ambas (#85). No ocupa hueco de `max_active`.
    async fn stop_run(&self, id: DownloadId) -> Reservation<'_> {
        let (reservation, handle) = {
            let mut queue = self.queue.lock().await;
            let reservation = self.reserve(id);
            let handle = queue.active.get_mut(&id).and_then(|run| {
                run.cancel.cancel();
                run.handle.take()
            });
            (reservation, handle)
        };
        if let Some(handle) = handle {
            // Una tarea que entró en pánico ya no tiene nada que esperar: el estado
            // en que quedó la unidad lo valida la orden después.
            let _ = handle.await;
        }
        reservation
    }

    /// Saca la unidad de la cola. Si no se puede guardar, la cola queda como estaba.
    async fn drop_unit(&self, id: DownloadId) -> Result<(), DownloadError> {
        let mut queue = self.queue.lock().await;
        let index = queue
            .units
            .iter()
            .position(|unit| unit.id == id)
            .ok_or(DownloadError::UnknownDownload)?;
        let removed = queue.units.remove(index);
        queue.version += 1;
        if let Err(error) = self.deps.store.save(&queue.to_file()) {
            queue.units.insert(index, removed);
            queue.version -= 1;
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{Arc, Mutex, OnceLock, Weak};

    use tokio_util::sync::CancellationToken;
    use wiremock::MockServer;

    use super::super::{ActiveRun, DownloadManager};
    use crate::download::events::ProgressPhase;
    use crate::download::paths::part_path;
    use crate::download::test_harness::{
        GIB, Harness, HookFactory, KIB, MIB, Options, QueueEnv, QueueFactory, SinkEvent, part,
        part_without_sha, request_count, serve, unit_of,
    };
    use crate::download::test_support::{cutting_server, pattern_bytes, stalling_server};
    use crate::download::{
        DownloadError, DownloadId, DownloadUnit, FailureKind, FileStatus, UnitState,
    };

    const SPLIT: [&str; 2] = ["m-00001-of-00002.gguf", "m-00002-of-00002.gguf"];
    /// Nadie escucha aquí: los tests que lo usan no deben llegar a pedir nada.
    const NO_SERVER: &str = "http://127.0.0.1:9";

    fn with_units(
        base: &str,
        units: impl FnOnce(&QueueEnv<'_>) -> Vec<DownloadUnit> + 'static,
    ) -> Harness {
        Harness::configured(
            base,
            Options {
                queue: Some(Box::new(units)),
                ..Options::default()
            },
        )
    }

    async fn state(h: &Harness, id: u64) -> UnitState {
        h.state_of(DownloadId(id)).await
    }

    #[tokio::test]
    async fn pausa_conserva_part() {
        let body = pattern_bytes(3 * MIB);
        let server = stalling_server(body.clone(), vec![0], MIB).await;
        let h = Harness::new(&server.base_url());
        let id = h.enqueue(vec![part("m.gguf", &body)]).await;
        h.wait_for_part("m.gguf", MIB as u64).await;
        let before = h.part_len("m.gguf");

        h.manager.pause(id).await.expect("pausar");

        assert_eq!(h.state_of(id).await, UnitState::Paused);
        assert_eq!(before, Some(MIB as u64));
        assert_eq!(h.part_len("m.gguf"), before);
    }

    #[tokio::test]
    async fn reanuda_desde_part() {
        let body = pattern_bytes(3 * MIB);
        let server = stalling_server(body.clone(), vec![0], MIB).await;
        let h = Harness::new(&server.base_url());
        let id = h.enqueue(vec![part("m.gguf", &body)]).await;
        h.wait_for_part("m.gguf", MIB as u64).await;
        h.manager.pause(id).await.expect("pausar");

        h.manager.resume(id).await.expect("reanudar");

        assert_eq!(h.finished(id).await, UnitState::Completed);
        assert_eq!(server.ranges(), [None, Some("bytes=1048576-".to_string())]);
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
    }

    #[tokio::test]
    async fn pausar_activa_arranca_siguiente() {
        let body = pattern_bytes(MIB);
        let server = stalling_server(body.clone(), vec![0], MIB / 2).await;
        let h = Harness::new(&server.base_url());
        let first = h.enqueue(vec![part("a.gguf", &body)]).await;
        let second = h.enqueue(vec![part("b.gguf", &body)]).await;
        h.wait_for_part("a.gguf", (MIB / 2) as u64).await;

        h.manager.pause(first).await.expect("pausar");

        assert_eq!(h.finished(second).await, UnitState::Completed);
        assert!(h.states(second).contains(&UnitState::Downloading));
        assert_eq!(h.state_of(first).await, UnitState::Paused);
    }

    #[tokio::test]
    async fn pausar_en_cola() {
        let server = MockServer::start().await;
        let h = Harness::configured(
            &server.uri(),
            Options {
                max_active: 0,
                ..Options::default()
            },
        );
        let id = h
            .enqueue(vec![part_without_sha("m.gguf", MIB as u64)])
            .await;

        h.manager.pause(id).await.expect("pausar");

        assert_eq!(h.state_of(id).await, UnitState::Paused);
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn pausar_en_cola_con_ejecucion_lanzada() {
        let server = MockServer::start().await;
        let h = Harness::new(&server.uri());
        let id = h
            .enqueue(vec![part_without_sha("m.gguf", MIB as u64)])
            .await;

        h.manager.pause(id).await.expect("pausar");

        assert_eq!(h.state_of(id).await, UnitState::Paused);
        assert_eq!(request_count(&server).await, 0);
        assert!(!h.part_exists("m.gguf"));
    }

    /// `max_active = 1`: A con una ejecución falsa en `active` y B en `Queued`,
    /// sin ejecución y con un `.part` suyo.
    async fn queued_behind_active(server: &MockServer) -> Harness {
        queued_behind_active_hooked(server, None).await
    }

    async fn queued_behind_active_hooked(
        server: &MockServer,
        hook: Option<HookFactory>,
    ) -> Harness {
        let body = pattern_bytes(KIB);
        let h = Harness::configured(
            &server.uri(),
            Options {
                hook,
                queue: Some(Box::new(move |env| {
                    vec![
                        unit_of(
                            1,
                            UnitState::Paused,
                            vec![env.file("a.gguf", &body, FileStatus::Pending)],
                        ),
                        unit_of(
                            2,
                            UnitState::Paused,
                            vec![env.file("b.gguf", &body, FileStatus::Pending)],
                        ),
                    ]
                })),
                ..Options::default()
            },
        );
        h.write_part("b.gguf", &pattern_bytes(100));
        let mut queue = h.manager.queue.lock().await;
        queue.unit_mut(DownloadId(1)).expect("A").state = UnitState::Downloading;
        queue.unit_mut(DownloadId(2)).expect("B").state = UnitState::Queued;
        queue.active.insert(
            DownloadId(1),
            ActiveRun {
                cancel: CancellationToken::new(),
                handle: None,
            },
        );
        drop(queue);
        h
    }

    /// A termina en el hueco entre `stop_run` y el final de la orden sobre B.
    async fn finish_active_a(h: &Harness) {
        h.manager
            .transition(DownloadId(1), UnitState::Verifying)
            .await
            .expect("verificar");
        h.manager
            .transition(DownloadId(1), UnitState::Completed)
            .await
            .expect("completar");
        h.manager.finish_run(DownloadId(1)).await;
    }

    async fn is_running(h: &Harness, id: u64) -> bool {
        h.manager
            .queue
            .lock()
            .await
            .active
            .contains_key(&DownloadId(id))
    }

    #[tokio::test]
    async fn stop_run_reserva_a_b_y_el_cierre_de_a_no_la_relanza_antes_de_cancelar() {
        let server = MockServer::start().await;
        let h = queued_behind_active(&server).await;
        let reservation = h.manager.stop_run(DownloadId(2)).await;

        finish_active_a(&h).await;

        assert!(!is_running(&h, 2).await);
        drop(reservation);
        h.manager.cancel(DownloadId(2)).await.expect("cancelar");
        assert_eq!(state(&h, 2).await, UnitState::Cancelled);
        assert!(!h.part_exists("b.gguf"));
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn stop_run_reserva_a_b_y_el_cierre_de_a_no_la_relanza_antes_de_pausar() {
        let server = MockServer::start().await;
        let h = queued_behind_active(&server).await;
        let reservation = h.manager.stop_run(DownloadId(2)).await;

        finish_active_a(&h).await;

        assert!(!is_running(&h, 2).await);
        drop(reservation);
        h.manager.pause(DownloadId(2)).await.expect("pausar");
        assert_eq!(state(&h, 2).await, UnitState::Paused);
        assert_eq!(request_count(&server).await, 0);
    }

    /// Comprueba, cuando B emite su estado final, si su reserva sigue tomada.
    struct ReservationProbe {
        manager: Arc<OnceLock<Weak<DownloadManager>>>,
        seen: Arc<Mutex<Vec<bool>>>,
    }

    impl ReservationProbe {
        fn new() -> Self {
            Self {
                manager: Arc::new(OnceLock::new()),
                seen: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn hook(&self, ending: UnitState) -> HookFactory {
            let manager = Arc::clone(&self.manager);
            let seen = Arc::clone(&self.seen);
            Box::new(move |_| {
                Arc::new(move |event| {
                    let SinkEvent::State(snapshot) = event else {
                        return;
                    };
                    if snapshot.id != DownloadId(2) || snapshot.state != ending {
                        return;
                    }
                    let held = manager
                        .get()
                        .and_then(Weak::upgrade)
                        .is_some_and(|manager| manager.reserved_ids().contains(&DownloadId(2)));
                    seen.lock().expect("sonda").push(held);
                })
            })
        }

        fn attach(&self, h: &Harness) {
            assert!(self.manager.set(Arc::downgrade(&h.manager)).is_ok());
        }

        fn seen(&self) -> Vec<bool> {
            self.seen.lock().expect("sonda").clone()
        }
    }

    #[tokio::test]
    async fn pausar_conserva_la_reserva_hasta_emitir_el_estado() {
        let server = MockServer::start().await;
        let probe = ReservationProbe::new();
        let h = queued_behind_active_hooked(&server, Some(probe.hook(UnitState::Paused))).await;
        probe.attach(&h);

        h.manager.pause(DownloadId(2)).await.expect("pausar");

        assert_eq!(probe.seen(), [true]);
        assert!(h.manager.reserved_ids().is_empty());
    }

    #[tokio::test]
    async fn cancelar_conserva_la_reserva_hasta_emitir_el_estado() {
        let server = MockServer::start().await;
        let probe = ReservationProbe::new();
        let h = queued_behind_active_hooked(&server, Some(probe.hook(UnitState::Cancelled))).await;
        probe.attach(&h);

        h.manager.cancel(DownloadId(2)).await.expect("cancelar");

        assert_eq!(probe.seen(), [true]);
        assert!(h.manager.reserved_ids().is_empty());
    }

    #[tokio::test]
    async fn la_reserva_no_ocupa_hueco_y_se_libera_al_soltarla() {
        let server = MockServer::start().await;
        let h = queued_behind_active(&server).await;
        let reservation = h.manager.stop_run(DownloadId(2)).await;
        {
            let queue = h.manager.queue.lock().await;
            assert_eq!(queue.active.len(), 1);
            assert_eq!(queue.next_waiting(&h.manager.reserved_ids()), None);
        }

        drop(reservation);

        let queue = h.manager.queue.lock().await;
        assert_eq!(
            queue.next_waiting(&h.manager.reserved_ids()),
            Some(DownloadId(2))
        );
    }

    #[tokio::test]
    async fn cancelar_borra_todo_lo_suyo() {
        let body = pattern_bytes(MIB);
        let server = stalling_server(body.clone(), vec![1], MIB / 2).await;
        let h = Harness::new(&server.base_url());
        let id = h
            .enqueue(SPLIT.iter().map(|name| part(name, &body)).collect())
            .await;
        h.wait_for_part(SPLIT[1], (MIB / 2) as u64).await;
        assert!(h.part_exists(SPLIT[0]));

        h.manager.cancel(id).await.expect("cancelar");

        assert_eq!(h.state_of(id).await, UnitState::Cancelled);
        for name in SPLIT {
            assert!(!h.part_exists(name), "{name}");
            assert!(!h.dest(name).exists(), "{name}");
        }
    }

    #[tokio::test]
    async fn cancelar_borra_finales_ya_renombrados() {
        let (first, second) = (pattern_bytes(KIB), pattern_bytes(2 * KIB));
        let (a, b) = (first.clone(), second.clone());
        let h = with_units(NO_SERVER, move |env| {
            vec![unit_of(
                1,
                UnitState::Paused,
                vec![
                    env.file(SPLIT[0], &a, FileStatus::Verified),
                    env.file(SPLIT[1], &b, FileStatus::Verified),
                ],
            )]
        });
        h.write_final(SPLIT[0], &first);
        h.write_part(SPLIT[1], &second);

        h.manager.cancel(DownloadId(1)).await.expect("cancelar");

        assert_eq!(state(&h, 1).await, UnitState::Cancelled);
        for name in SPLIT {
            assert!(!h.part_exists(name), "{name}");
            assert!(!h.dest(name).exists(), "{name}");
        }
    }

    #[tokio::test]
    async fn cancelar_respeta_preexistentes() {
        let (first, second) = (pattern_bytes(KIB), pattern_bytes(2 * KIB));
        let (a, b) = (first.clone(), second.clone());
        let h = with_units(NO_SERVER, move |env| {
            vec![unit_of(
                1,
                UnitState::Paused,
                vec![
                    env.file(SPLIT[0], &a, FileStatus::Preexisting),
                    env.file(SPLIT[1], &b, FileStatus::Pending),
                ],
            )]
        });
        h.write_final(SPLIT[0], &first);
        h.write_part(SPLIT[1], &second[..KIB]);

        h.manager.cancel(DownloadId(1)).await.expect("cancelar");

        assert_eq!(state(&h, 1).await, UnitState::Cancelled);
        assert_eq!(fs::read(h.dest(SPLIT[0])).expect("final"), first);
        assert!(!h.part_exists(SPLIT[1]));
    }

    #[tokio::test]
    async fn cancelar_pausada() {
        let body = pattern_bytes(KIB);
        let b = body.clone();
        let h = with_units(NO_SERVER, move |env| {
            vec![unit_of(
                1,
                UnitState::Paused,
                vec![env.file("m.gguf", &b, FileStatus::Pending)],
            )]
        });
        h.write_part("m.gguf", &body[..100]);

        h.manager.cancel(DownloadId(1)).await.expect("cancelar");

        assert_eq!(state(&h, 1).await, UnitState::Cancelled);
        assert!(!h.part_exists("m.gguf"));
    }

    #[tokio::test]
    async fn cancelar_cuando_ya_completo_no_borra() {
        let body = pattern_bytes(KIB);
        let b = body.clone();
        let h = with_units(NO_SERVER, move |env| {
            vec![unit_of(
                1,
                UnitState::Paused,
                vec![env.file("m.gguf", &b, FileStatus::Verified)],
            )]
        });
        h.write_part("m.gguf", &body);
        let id = DownloadId(1);
        // Una ejecución en `Verifying` que ya no puede observar la cancelación:
        // termina en `Completed` mientras la orden la espera.
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let manager = Arc::clone(&h.manager);
        let (dest, part) = (h.dest("m.gguf"), part_path(&h.dest("m.gguf")));
        let handle = tokio::spawn(async move {
            token.cancelled().await;
            fs::rename(part, dest).expect("renombrar");
            manager
                .transition(id, UnitState::Completed)
                .await
                .expect("completar");
            manager.finish_run(id).await;
        });
        {
            let mut queue = h.manager.queue.lock().await;
            queue.unit_mut(id).expect("unidad").state = UnitState::Verifying;
            queue.active.insert(
                id,
                ActiveRun {
                    cancel,
                    handle: Some(handle),
                },
            );
        }

        let result = h.manager.cancel(id).await;

        assert!(
            matches!(result, Err(DownloadError::InvalidTransition)),
            "{result:?}"
        );
        assert_eq!(state(&h, 1).await, UnitState::Completed);
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
    }

    #[tokio::test]
    async fn reintentar_tras_fallo_red() {
        let body = pattern_bytes(2 * MIB);
        let server = cutting_server(body.clone(), Vec::new(), false).await;
        let b = body.clone();
        let h = with_units(&server.base_url(), move |env| {
            vec![unit_of(
                1,
                UnitState::Failed(FailureKind::Network),
                vec![env.file("m.gguf", &b, FileStatus::Pending)],
            )]
        });
        h.write_part("m.gguf", &body[..MIB]);

        h.manager.retry(DownloadId(1)).await.expect("reintentar");

        assert_eq!(h.finished(DownloadId(1)).await, UnitState::Completed);
        assert_eq!(h.states(DownloadId(1))[0], UnitState::Queued);
        assert_eq!(server.ranges(), [Some("bytes=1048576-".to_string())]);
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
    }

    #[tokio::test]
    async fn reanudar_solo_desde_pausa_y_reintentar_solo_desde_fallo() {
        let body = pattern_bytes(KIB);
        let (a, b) = (body.clone(), body);
        let h = with_units(NO_SERVER, move |env| {
            vec![
                unit_of(
                    1,
                    UnitState::Failed(FailureKind::Network),
                    vec![env.file("a.gguf", &a, FileStatus::Pending)],
                ),
                unit_of(
                    2,
                    UnitState::Paused,
                    vec![env.file("b.gguf", &b, FileStatus::Pending)],
                ),
            ]
        });

        let resume_failed = h.manager.resume(DownloadId(1)).await;
        let retry_paused = h.manager.retry(DownloadId(2)).await;

        assert!(matches!(
            resume_failed,
            Err(DownloadError::InvalidTransition)
        ));
        assert!(matches!(
            retry_paused,
            Err(DownloadError::InvalidTransition)
        ));
        assert_eq!(state(&h, 1).await, UnitState::Failed(FailureKind::Network));
        assert_eq!(state(&h, 2).await, UnitState::Paused);
    }

    #[tokio::test]
    async fn quitar_solo_terminales() {
        let body = pattern_bytes(MIB);
        let server = stalling_server(body.clone(), vec![0], MIB / 2).await;
        let h = Harness::new(&server.base_url());
        let id = h.enqueue(vec![part("m.gguf", &body)]).await;
        h.wait_for_part("m.gguf", (MIB / 2) as u64).await;

        let result = h.manager.remove(id).await;

        assert!(matches!(result, Err(DownloadError::InvalidTransition)));
        assert_eq!(h.manager.list().await.len(), 1);
    }

    #[tokio::test]
    async fn quitar_fallida_borra_part() {
        let body = pattern_bytes(KIB);
        let b = body.clone();
        let h = with_units(NO_SERVER, move |env| {
            vec![unit_of(
                1,
                UnitState::Failed(FailureKind::Network),
                vec![env.file("m.gguf", &b, FileStatus::Pending)],
            )]
        });
        h.write_part("m.gguf", &body[..100]);

        h.manager.remove(DownloadId(1)).await.expect("quitar");

        assert!(h.manager.list().await.is_empty());
        assert!(!h.part_exists("m.gguf"));
    }

    #[tokio::test]
    async fn quitar_completada_conserva_final() {
        let body = pattern_bytes(KIB);
        let b = body.clone();
        let h = with_units(NO_SERVER, move |env| {
            vec![unit_of(
                1,
                UnitState::Completed,
                vec![env.file("m.gguf", &b, FileStatus::Verified)],
            )]
        });
        h.write_final("m.gguf", &body);

        h.manager.remove(DownloadId(1)).await.expect("quitar");

        assert!(h.manager.list().await.is_empty());
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
    }

    #[tokio::test]
    async fn quitar_persiste_la_cola_sin_la_unidad() {
        let body = pattern_bytes(KIB);
        let h = with_units(NO_SERVER, move |env| {
            vec![unit_of(
                1,
                UnitState::Cancelled,
                vec![env.file("m.gguf", &body, FileStatus::Pending)],
            )]
        });

        h.manager.remove(DownloadId(1)).await.expect("quitar");

        let saved = h.store.last_saved().expect("guardado");
        assert!(saved.units.is_empty());
        assert_eq!(saved.next_id, 2);
    }

    #[tokio::test]
    async fn quitar_fallida_borra_partes_ya_renombradas() {
        let (first, second) = (pattern_bytes(KIB), pattern_bytes(2 * KIB));
        let (a, b) = (first.clone(), second.clone());
        let h = with_units(NO_SERVER, move |env| {
            vec![unit_of(
                1,
                UnitState::Failed(FailureKind::Network),
                vec![
                    env.file(SPLIT[0], &a, FileStatus::Verified),
                    env.file(SPLIT[1], &b, FileStatus::Verified),
                ],
            )]
        });
        h.write_final(SPLIT[0], &first);
        h.write_part(SPLIT[1], &second);

        h.manager.remove(DownloadId(1)).await.expect("quitar");

        assert!(h.manager.list().await.is_empty());
        assert!(!h.dest(SPLIT[0]).exists());
        assert!(!h.part_exists(SPLIT[1]));
    }

    #[tokio::test]
    async fn quitar_fallida_respeta_preexistentes_y_ajenos() {
        let (first, second) = (pattern_bytes(KIB), pattern_bytes(2 * KIB));
        let (a, b) = (first.clone(), second.clone());
        let h = with_units(NO_SERVER, move |env| {
            vec![unit_of(
                1,
                UnitState::Failed(FailureKind::DestinationExists),
                vec![
                    env.file(SPLIT[0], &a, FileStatus::Preexisting),
                    env.file(SPLIT[1], &b, FileStatus::Pending),
                ],
            )]
        });
        h.write_final(SPLIT[0], &first);
        h.write_final(SPLIT[1], &second);

        h.manager.remove(DownloadId(1)).await.expect("quitar");

        assert!(h.manager.list().await.is_empty());
        assert_eq!(fs::read(h.dest(SPLIT[0])).expect("final"), first);
        assert_eq!(fs::read(h.dest(SPLIT[1])).expect("final"), second);
    }

    #[tokio::test]
    async fn quitar_no_borra_ficheros_de_otra_unidad() {
        let (first, second) = (pattern_bytes(KIB), pattern_bytes(2 * KIB));
        let (a, b) = (first.clone(), second.clone());
        let h = with_units(NO_SERVER, move |env| {
            vec![
                unit_of(
                    1,
                    UnitState::Failed(FailureKind::Network),
                    vec![
                        env.file(SPLIT[0], &a, FileStatus::Verified),
                        env.file(SPLIT[1], &b, FileStatus::Pending),
                    ],
                ),
                unit_of(
                    2,
                    UnitState::Completed,
                    vec![
                        env.file(SPLIT[0], &a, FileStatus::Preexisting),
                        env.file(SPLIT[1], &b, FileStatus::Verified),
                    ],
                ),
            ]
        });
        h.write_final(SPLIT[0], &first);
        h.write_final(SPLIT[1], &second);

        h.manager.remove(DownloadId(1)).await.expect("quitar");

        let units = h.manager.list().await;
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].state, UnitState::Completed);
        assert_eq!(fs::read(h.dest(SPLIT[0])).expect("final"), first);
        assert_eq!(fs::read(h.dest(SPLIT[1])).expect("final"), second);
    }

    #[tokio::test]
    async fn quitar_no_borra_part_de_otra_unidad() {
        let body = pattern_bytes(MIB);
        let server = cutting_server(body.clone(), Vec::new(), false).await;
        let (first, second) = (body.clone(), body.clone());
        let h = with_units(&server.base_url(), move |env| {
            vec![
                unit_of(
                    1,
                    UnitState::Failed(FailureKind::Network),
                    vec![
                        env.file(SPLIT[0], &first, FileStatus::Verified),
                        env.file(SPLIT[1], &second, FileStatus::Pending),
                    ],
                ),
                unit_of(
                    2,
                    UnitState::Downloading,
                    vec![
                        env.file(SPLIT[0], &first, FileStatus::Preexisting),
                        env.file(SPLIT[1], &second, FileStatus::Pending),
                    ],
                ),
            ]
        });
        h.write_final(SPLIT[0], &body);
        h.write_part(SPLIT[1], &body[..64 * KIB]);

        h.manager.remove(DownloadId(1)).await.expect("quitar");

        assert_eq!(fs::read(h.dest(SPLIT[0])).expect("final"), body);
        assert_eq!(h.part_len(SPLIT[1]), Some(65_536));
        h.manager.resume(DownloadId(2)).await.expect("reanudar");
        assert_eq!(h.finished(DownloadId(2)).await, UnitState::Completed);
        assert_eq!(server.ranges(), [Some("bytes=65536-".to_string())]);
    }

    #[tokio::test]
    async fn cancelar_no_borra_ficheros_de_otra_unidad() {
        let (first, second) = (pattern_bytes(16), vec![7u8; 16]);
        let (a, b) = (first.clone(), second.clone());
        let h = with_units(NO_SERVER, move |env| {
            vec![
                unit_of(
                    1,
                    UnitState::Paused,
                    vec![
                        env.file(SPLIT[0], &a, FileStatus::Verified),
                        env.file(SPLIT[1], &b, FileStatus::Pending),
                    ],
                ),
                unit_of(
                    2,
                    UnitState::Completed,
                    vec![env.file(SPLIT[0], &a, FileStatus::Verified)],
                ),
            ]
        });
        h.write_final(SPLIT[0], &first);
        h.write_part(SPLIT[1], &second[..8]);

        h.manager.cancel(DownloadId(1)).await.expect("cancelar");

        assert_eq!(state(&h, 1).await, UnitState::Cancelled);
        assert_eq!(fs::read(h.dest(SPLIT[0])).expect("final"), first);
        assert!(!h.part_exists(SPLIT[1]));
        assert_eq!(state(&h, 2).await, UnitState::Completed);
    }

    #[tokio::test]
    async fn cancelar_no_borra_part_de_unidad_viva() {
        let body = pattern_bytes(MIB);
        let server = cutting_server(body.clone(), Vec::new(), false).await;
        let b = body.clone();
        let h = with_units(&server.base_url(), move |env| {
            vec![
                unit_of(
                    1,
                    UnitState::Paused,
                    vec![env.file("m.gguf", &b, FileStatus::Pending)],
                ),
                unit_of(
                    2,
                    UnitState::Paused,
                    vec![env.file("m.gguf", &b, FileStatus::Pending)],
                ),
            ]
        });
        h.write_part("m.gguf", &body[..64 * KIB]);

        h.manager.cancel(DownloadId(1)).await.expect("cancelar");

        assert_eq!(state(&h, 1).await, UnitState::Cancelled);
        assert_eq!(h.part_len("m.gguf"), Some(65_536));
        h.manager.resume(DownloadId(2)).await.expect("reanudar");
        assert_eq!(h.finished(DownloadId(2)).await, UnitState::Completed);
        assert_eq!(server.ranges(), [Some("bytes=65536-".to_string())]);
    }

    #[tokio::test]
    async fn cancelar_tras_otra_cancelada_borra_su_part() {
        let body = pattern_bytes(256 * KIB);
        let server = stalling_server(body.clone(), vec![0, 1], 128 * KIB).await;
        let h = Harness::new(&server.base_url());
        let first = h.enqueue(vec![part("m.gguf", &body)]).await;
        h.wait_for_part("m.gguf", (128 * KIB) as u64).await;
        h.manager.cancel(first).await.expect("cancelar la primera");
        assert!(!h.part_exists("m.gguf"));
        let second = h.enqueue(vec![part("m.gguf", &body)]).await;
        h.wait_for_part("m.gguf", (128 * KIB) as u64).await;

        h.manager.cancel(second).await.expect("cancelar la segunda");

        assert_eq!(h.state_of(second).await, UnitState::Cancelled);
        assert!(!h.part_exists("m.gguf"));
        assert!(!h.dest("m.gguf").exists());
        assert_eq!(h.state_of(first).await, UnitState::Cancelled);
    }

    #[tokio::test]
    async fn cancelar_tras_otra_completada_sin_final_borra_su_part() {
        let body = pattern_bytes(256 * KIB);
        let server = stalling_server(body.clone(), vec![0], 128 * KIB).await;
        let b = body.clone();
        let h = with_units(&server.base_url(), move |env| {
            vec![unit_of(
                1,
                UnitState::Completed,
                vec![env.file("m.gguf", &b, FileStatus::Verified)],
            )]
        });
        let second = h.enqueue(vec![part("m.gguf", &body)]).await;
        h.wait_for_part("m.gguf", (128 * KIB) as u64).await;

        h.manager.cancel(second).await.expect("cancelar");

        assert_eq!(h.state_of(second).await, UnitState::Cancelled);
        assert!(!h.part_exists("m.gguf"));
        assert!(!h.dest("m.gguf").exists());
        assert_eq!(state(&h, 1).await, UnitState::Completed);
    }

    #[tokio::test]
    async fn orden_sobre_id_inexistente() {
        let h = Harness::new(NO_SERVER);
        let unknown = DownloadId(999);

        let results = [
            h.manager.pause(unknown).await,
            h.manager.resume(unknown).await,
            h.manager.cancel(unknown).await,
            h.manager.retry(unknown).await,
            h.manager.remove(unknown).await,
        ];

        for result in results {
            assert!(
                matches!(result, Err(DownloadError::UnknownDownload)),
                "{result:?}"
            );
        }
    }

    #[tokio::test]
    async fn restaurar_y_reanudar() {
        let body = pattern_bytes(6 * MIB);
        let server = cutting_server(body.clone(), Vec::new(), false).await;
        let b = body.clone();
        let h = with_units(&server.base_url(), move |env| {
            vec![unit_of(
                1,
                UnitState::Downloading,
                vec![env.file("m.gguf", &b, FileStatus::Pending)],
            )]
        });
        h.write_part("m.gguf", &body[..4 * MIB]);

        assert_eq!(state(&h, 1).await, UnitState::Paused);
        assert!(server.ranges().is_empty());

        h.manager.resume(DownloadId(1)).await.expect("reanudar");

        assert_eq!(h.finished(DownloadId(1)).await, UnitState::Completed);
        assert_eq!(server.ranges(), [Some("bytes=4194304-".to_string())]);
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
    }

    #[tokio::test]
    async fn reanudar_tras_caida_en_finalize_completa() {
        let body = pattern_bytes(KIB);
        let server = MockServer::start().await;
        let (a, b) = (body.clone(), body.clone());
        let h = with_units(&server.uri(), move |env| {
            vec![unit_of(
                1,
                UnitState::Downloading,
                vec![
                    env.file(SPLIT[0], &a, FileStatus::Verified),
                    env.file(SPLIT[1], &b, FileStatus::Verified),
                ],
            )]
        });
        h.write_final(SPLIT[0], &body);
        h.write_part(SPLIT[1], &body);
        assert_eq!(state(&h, 1).await, UnitState::Paused);

        h.manager.resume(DownloadId(1)).await.expect("reanudar");

        assert_eq!(h.finished(DownloadId(1)).await, UnitState::Completed);
        assert_eq!(request_count(&server).await, 0);
        for name in SPLIT {
            assert_eq!(fs::read(h.dest(name)).expect("final"), body, "{name}");
            assert!(!h.part_exists(name), "{name}");
        }
    }

    #[tokio::test]
    async fn verificada_sin_fichero_vuelve_a_pending() {
        let body = pattern_bytes(MIB);
        let server = cutting_server(body.clone(), Vec::new(), false).await;
        let b = body.clone();
        let h = with_units(&server.base_url(), move |env| {
            vec![unit_of(
                1,
                UnitState::Paused,
                vec![env.file("m.gguf", &b, FileStatus::Verified)],
            )]
        });

        h.manager.resume(DownloadId(1)).await.expect("reanudar");

        assert_eq!(h.finished(DownloadId(1)).await, UnitState::Completed);
        assert_eq!(server.ranges(), [None]);
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
    }

    /// Unidad dividida en 2 partes de 1 MiB: la 1.ª `Verified` sin ficheros y la
    /// 2.ª `Pending` sin `.part`.
    fn lost_first_part(disk: u64, server: &MockServer) -> Harness {
        let body = pattern_bytes(MIB);
        let (a, b) = (body.clone(), body);
        let queue: QueueFactory = Box::new(move |env| {
            vec![unit_of(
                1,
                UnitState::Paused,
                vec![
                    env.file(SPLIT[0], &a, FileStatus::Verified),
                    env.file(SPLIT[1], &b, FileStatus::Pending),
                ],
            )]
        });
        Harness::configured(
            &server.uri(),
            Options {
                disk,
                queue: Some(queue),
                ..Options::default()
            },
        )
    }

    #[tokio::test]
    async fn verificada_sin_fichero_espacio_justo_completa() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        for name in SPLIT {
            serve(&server, name, &body).await;
        }
        let h = lost_first_part(2 * MIB as u64 + GIB, &server);

        h.manager.resume(DownloadId(1)).await.expect("reanudar");

        assert_eq!(h.finished(DownloadId(1)).await, UnitState::Completed);
        let requests = server.received_requests().await.expect("peticiones");
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|r| !r.headers.contains_key("range")));
    }

    #[tokio::test]
    async fn verificada_sin_fichero_cuenta_para_el_espacio() {
        let server = MockServer::start().await;
        let h = lost_first_part(2 * MIB as u64 + GIB - 1, &server);

        h.manager.resume(DownloadId(1)).await.expect("reanudar");

        assert_eq!(
            h.finished(DownloadId(1)).await,
            UnitState::Failed(FailureKind::InsufficientSpace {
                needed: 1_075_838_976,
                available: 1_075_838_975,
            })
        );
        assert_eq!(request_count(&server).await, 0);
        let saved = h.store.last_saved().expect("guardado");
        assert_eq!(saved.units[0].files[0].status, FileStatus::Pending);
    }

    #[tokio::test]
    async fn pendiente_sin_final_cuenta_para_el_espacio() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        let h = Harness::configured(
            &server.uri(),
            Options {
                disk: 0,
                queue: Some(Box::new(move |env| {
                    vec![unit_of(
                        1,
                        UnitState::Paused,
                        vec![env.file("m.gguf", &body, FileStatus::Pending)],
                    )]
                })),
                ..Options::default()
            },
        );

        h.manager.resume(DownloadId(1)).await.expect("reanudar");

        assert_eq!(
            h.finished(DownloadId(1)).await,
            UnitState::Failed(FailureKind::InsufficientSpace {
                needed: 1_074_790_400,
                available: 0,
            })
        );
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn quitar_completada_y_reencolar_sin_espacio() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        let b = body.clone();
        let h = Harness::configured(
            &server.uri(),
            Options {
                disk: 0,
                queue: Some(Box::new(move |env| {
                    vec![unit_of(
                        1,
                        UnitState::Completed,
                        vec![env.file("m.gguf", &b, FileStatus::Verified)],
                    )]
                })),
                ..Options::default()
            },
        );
        h.write_final("m.gguf", &body);
        h.manager.remove(DownloadId(1)).await.expect("quitar");

        let id = h.enqueue(vec![part("m.gguf", &body)]).await;

        assert_eq!(h.finished(id).await, UnitState::Completed);
        assert_eq!(h.file_status(id, 0).await, FileStatus::Preexisting);
        assert_eq!(request_count(&server).await, 0);
        assert_eq!(fs::read(h.dest("m.gguf")).expect("final"), body);
    }

    #[tokio::test]
    async fn candidata_desaparecida_falla_io_y_reintento_descarga() {
        let body = pattern_bytes(MIB);
        let server = MockServer::start().await;
        for name in SPLIT {
            serve(&server, name, &body).await;
        }
        let hook: HookFactory = Box::new(|models_dir| {
            let victim = models_dir.join("org").join("model").join(SPLIT[0]);
            Arc::new(move |event| {
                if matches!(event, SinkEvent::Progress(p) if p.phase == ProgressPhase::Download) {
                    // El borrado es parte del escenario: si ya no existe, da igual.
                    let _ = fs::remove_file(&victim);
                }
            })
        });
        let h = Harness::configured(
            &server.uri(),
            Options {
                disk: 2 * MIB as u64 + GIB,
                hook: Some(hook),
                ..Options::default()
            },
        );
        h.write_final(SPLIT[0], &body);
        let id = h
            .enqueue(SPLIT.iter().map(|name| part(name, &body)).collect())
            .await;

        let failed = h.finished(id).await;

        let UnitState::Failed(FailureKind::Io { message }) = failed else {
            panic!("se esperaba Failed(Io), llegó {failed:?}");
        };
        assert!(!message.contains(&*h.dir.path().to_string_lossy()));
        assert!(h.part_exists(SPLIT[1]));
        assert_eq!(request_count(&server).await, 1);

        h.manager.retry(id).await.expect("reintentar");

        assert_eq!(h.finished(id).await, UnitState::Completed);
        let requests = server.received_requests().await.expect("peticiones");
        assert_eq!(requests.len(), 2);
        assert!(!requests[1].headers.contains_key("range"));
        for name in SPLIT {
            assert_eq!(fs::read(h.dest(name)).expect("final"), body, "{name}");
            assert!(!h.part_exists(name), "{name}");
        }
    }
}

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use super::clock::Clock;

const SPEED_WINDOW: Duration = Duration::from_secs(5);
const EVENT_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_sec: u64,
    pub eta_secs: Option<u64>,
}

/// Mide la velocidad sobre una ventana deslizante. Los bytes que ya había
/// antes de empezar (reanudación) cuentan como hechos pero no como velocidad.
pub struct ProgressTracker {
    total: u64,
    downloaded: u64,
    /// Instante y bytes acumulados desde que empezó este intento.
    samples: VecDeque<(Duration, u64)>,
    received: u64,
}

impl ProgressTracker {
    pub fn new(total: u64, already_done: u64) -> Self {
        Self {
            total,
            downloaded: already_done,
            samples: VecDeque::new(),
            received: 0,
        }
    }

    pub fn record(&mut self, now: Duration, bytes: u64) {
        self.downloaded = self.downloaded.saturating_add(bytes);
        self.received = self.received.saturating_add(bytes);
        self.samples.push_back((now, self.received));
        self.drop_samples_outside_window(now);
    }

    pub fn snapshot(&self, now: Duration) -> Progress {
        let bytes_per_sec = self.speed(now);
        Progress {
            downloaded: self.downloaded,
            total: self.total,
            bytes_per_sec,
            eta_secs: self.eta(bytes_per_sec),
        }
    }

    fn drop_samples_outside_window(&mut self, now: Duration) {
        let oldest_allowed = now.saturating_sub(SPEED_WINDOW);
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| *at < oldest_allowed)
        {
            self.samples.pop_front();
        }
    }

    fn speed(&self, now: Duration) -> u64 {
        let oldest_allowed = now.saturating_sub(SPEED_WINDOW);
        let mut in_window = self.samples.iter().filter(|(at, _)| *at >= oldest_allowed);
        let (Some((first_at, first_bytes)), Some((last_at, last_bytes))) =
            (in_window.next(), in_window.next_back())
        else {
            return 0;
        };
        let elapsed_nanos = last_at.saturating_sub(*first_at).as_nanos();
        if elapsed_nanos == 0 {
            return 0;
        }
        let bytes = u128::from(last_bytes - first_bytes);
        u64::try_from(bytes * 1_000_000_000 / elapsed_nanos).unwrap_or(u64::MAX)
    }

    fn eta(&self, bytes_per_sec: u64) -> Option<u64> {
        let remaining = self.total.saturating_sub(self.downloaded);
        if remaining == 0 {
            return Some(0);
        }
        (bytes_per_sec > 0).then(|| remaining.div_ceil(bytes_per_sec))
    }
}

/// Deja pasar como máximo un evento por intervalo, más los forzados.
pub struct Throttle {
    interval: Duration,
    last_emit: Option<Duration>,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_emit: None,
        }
    }

    pub fn should_emit(&mut self, now: Duration) -> bool {
        let due = self
            .last_emit
            .is_none_or(|last| now.saturating_sub(last) >= self.interval);
        if due {
            self.last_emit = Some(now);
        }
        due
    }

    /// Emisión obligatoria (evento final): siempre `true`, y reinicia el intervalo.
    pub fn force(&mut self, now: Duration) -> bool {
        self.last_emit = Some(now);
        true
    }
}

pub struct ProgressReporter {
    tracker: ProgressTracker,
    throttle: Throttle,
    clock: Arc<dyn Clock>,
}

impl ProgressReporter {
    pub fn new(total: u64, already_done: u64, clock: Arc<dyn Clock>) -> Self {
        Self {
            tracker: ProgressTracker::new(total, already_done),
            throttle: Throttle::new(EVENT_INTERVAL),
            clock,
        }
    }

    /// Registra un bloque; devuelve el progreso solo si toca emitir evento.
    pub fn record(&mut self, bytes: u64) -> Option<Progress> {
        let now = self.clock.now();
        self.tracker.record(now, bytes);
        self.throttle
            .should_emit(now)
            .then(|| self.tracker.snapshot(now))
    }

    pub fn finish(&mut self) -> Progress {
        let now = self.clock.now();
        self.throttle.force(now);
        self.tracker.snapshot(now)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::download::clock::FakeClock;

    const MIB: u64 = 1024 * 1024;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Registra `count` bloques de `bytes`, uno cada `step_ms`, y devuelve el instante final.
    fn feed(
        tracker: &mut ProgressTracker,
        from_ms: u64,
        count: u64,
        step_ms: u64,
        bytes: u64,
    ) -> u64 {
        let mut t = from_ms;
        for _ in 0..count {
            t += step_ms;
            tracker.record(ms(t), bytes);
        }
        t
    }

    #[test]
    fn velocidad_constante() {
        let mut tracker = ProgressTracker::new(100 * MIB, 0);
        let end = feed(&mut tracker, 0, 20, 100, MIB);
        assert_eq!(tracker.snapshot(ms(end)).bytes_per_sec, 10 * MIB);
    }

    #[test]
    fn velocidad_ventana_5s_olvida_pasado() {
        let mut tracker = ProgressTracker::new(1000 * MIB, 0);
        let mid = feed(&mut tracker, 0, 50, 100, MIB);
        let end = feed(&mut tracker, mid, 60, 100, MIB / 10);
        let speed = tracker.snapshot(ms(end)).bytes_per_sec;
        assert!(speed.abs_diff(MIB) <= MIB / 100, "velocidad {speed}");
    }

    #[test]
    fn velocidad_sin_muestras() {
        let tracker = ProgressTracker::new(MIB, 0);
        let progress = tracker.snapshot(ms(0));
        assert_eq!(progress.bytes_per_sec, 0);
        assert_eq!(progress.eta_secs, None);
    }

    #[test]
    fn velocidad_cae_a_cero_tras_un_parón_largo() {
        let mut tracker = ProgressTracker::new(100 * MIB, 0);
        let end = feed(&mut tracker, 0, 20, 100, MIB);
        assert_eq!(tracker.snapshot(ms(end + 6000)).bytes_per_sec, 0);
    }

    #[test]
    fn eta_calculada() {
        let mut tracker = ProgressTracker::new(100 * MIB, 30 * MIB);
        let end = feed(&mut tracker, 0, 10, 100, MIB);
        let progress = tracker.snapshot(ms(end));
        assert_eq!(progress.downloaded, 40 * MIB);
        assert_eq!(progress.eta_secs, Some(6));
    }

    #[test]
    fn eta_redondea_hacia_arriba() {
        let mut tracker = ProgressTracker::new(100 * MIB, 85 * MIB - 10 * MIB);
        let end = feed(&mut tracker, 0, 10, 100, MIB);
        let progress = tracker.snapshot(ms(end));
        assert_eq!(progress.downloaded, 85 * MIB);
        assert_eq!(progress.eta_secs, Some(2));
    }

    #[test]
    fn eta_cero_al_terminar() {
        let tracker = ProgressTracker::new(MIB, MIB);
        assert_eq!(tracker.snapshot(ms(0)).eta_secs, Some(0));
    }

    #[test]
    fn reanudacion_no_infla_velocidad() {
        let four_gib = 4 * 1024 * MIB;
        let mut tracker = ProgressTracker::new(five_gib(), four_gib);
        let end = feed(&mut tracker, 0, 2, 1000, MIB);
        let progress = tracker.snapshot(ms(end));
        assert_eq!(progress.bytes_per_sec, MIB);
        assert_eq!(progress.downloaded, four_gib + 2 * MIB);
    }

    fn five_gib() -> u64 {
        5 * 1024 * MIB
    }

    #[test]
    fn progreso_mas_de_50gb() {
        let mut tracker = ProgressTracker::new(64_424_509_440, 60_000_000_000 - 100 * MIB);
        let end = feed(&mut tracker, 0, 10, 100, 10 * MIB);
        let progress = tracker.snapshot(ms(end));
        assert_eq!(progress.downloaded, 60_000_000_000);
        assert_eq!(progress.bytes_per_sec, 100 * MIB);
        assert_eq!(progress.eta_secs, Some(43));
    }

    #[test]
    fn throttle_limita_a_4hz() {
        let mut throttle = Throttle::new(ms(250));
        let emitted = (1..=100)
            .filter(|i| throttle.should_emit(ms(i * 10)))
            .count();
        assert!((4..=5).contains(&emitted), "emitidos {emitted}");
    }

    #[test]
    fn throttle_primer_evento_inmediato() {
        assert!(Throttle::new(ms(250)).should_emit(ms(0)));
    }

    #[test]
    fn throttle_force_siempre() {
        let mut throttle = Throttle::new(ms(250));
        assert!(throttle.should_emit(ms(0)));
        assert!(!throttle.should_emit(ms(10)));
        assert!(throttle.force(ms(10)));
    }

    #[test]
    fn throttle_tras_force_respeta_el_intervalo() {
        let mut throttle = Throttle::new(ms(250));
        throttle.force(ms(100));
        assert!(!throttle.should_emit(ms(200)));
        assert!(throttle.should_emit(ms(350)));
    }

    #[test]
    fn reporter_ca1_10mib() {
        const BLOCK: u64 = 64 * 1024;
        let total = 10 * MIB;
        let clock = Arc::new(FakeClock::new());
        let mut reporter = ProgressReporter::new(total, 0, clock.clone());

        let mut events = Vec::new();
        for _ in 0..(total / BLOCK) {
            clock.advance(ms(50));
            events.extend(reporter.record(BLOCK));
        }
        events.push(reporter.finish());

        assert!(events.len() <= 34, "eventos {}", events.len());
        let last = events.last().expect("hay al menos el evento final");
        assert_eq!(last.downloaded, total);
        assert_eq!(last.total, total);
        assert!(last.bytes_per_sec.abs_diff(1_310_720) <= 13_107);
        assert_eq!(last.eta_secs, Some(0));
    }

    #[test]
    fn reporter_emite_el_primer_bloque() {
        let clock = Arc::new(FakeClock::new());
        let mut reporter = ProgressReporter::new(MIB, 0, clock);
        assert!(reporter.record(1024).is_some());
    }
}

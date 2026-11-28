use std::time::{Duration, Instant};

/// Reloj monotónico: devuelve el tiempo transcurrido desde un origen propio.
/// Se inyecta para que los tests no dependan del reloj real.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}

pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

#[cfg(test)]
pub use fakes::{FakeClock, SteppingClock};

#[cfg(test)]
mod fakes {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use super::Clock;

    fn nanos(duration: Duration) -> u64 {
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
    }

    /// Solo avanza cuando el test lo pide.
    #[derive(Default)]
    pub struct FakeClock {
        nanos: AtomicU64,
    }

    impl FakeClock {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn advance(&self, by: Duration) {
            self.nanos.fetch_add(nanos(by), Ordering::SeqCst);
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Duration {
            Duration::from_nanos(self.nanos.load(Ordering::SeqCst))
        }
    }

    /// Avanza un paso fijo tras cada lectura: la primera devuelve cero.
    pub struct SteppingClock {
        nanos: AtomicU64,
        step: u64,
    }

    impl SteppingClock {
        pub fn new(step: Duration) -> Self {
            Self {
                nanos: AtomicU64::new(0),
                step: nanos(step),
            }
        }
    }

    impl Clock for SteppingClock {
        fn now(&self) -> Duration {
            Duration::from_nanos(self.nanos.fetch_add(self.step, Ordering::SeqCst))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn fake_clock_solo_avanza_a_demanda() {
        let clock = FakeClock::new();
        assert_eq!(clock.now(), Duration::ZERO);
        clock.advance(Duration::from_millis(50));
        clock.advance(Duration::from_millis(50));
        assert_eq!(clock.now(), Duration::from_millis(100));
    }

    #[test]
    fn stepping_clock_avanza_un_paso_en_cada_lectura() {
        let clock = SteppingClock::new(Duration::from_millis(50));
        assert_eq!(clock.now(), Duration::ZERO);
        assert_eq!(clock.now(), Duration::from_millis(50));
        assert_eq!(clock.now(), Duration::from_millis(100));
    }

    #[test]
    fn system_clock_es_monotono() {
        let clock = SystemClock::new();
        let first = clock.now();
        assert!(clock.now() >= first);
    }
}

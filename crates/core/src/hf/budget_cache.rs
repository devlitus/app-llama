//! Caché del presupuesto de memoria: el hardware se detecta una vez por sesión.
use std::future::Future;

use tokio::sync::OnceCell;

use super::fit::MemoryBudget;
use crate::runtime::HardwareInfo;

#[derive(Debug, Default)]
pub struct BudgetCache {
    budget: OnceCell<MemoryBudget>,
}

impl BudgetCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Si la detección falla se devuelve `unknown()` sin guardarlo, para reintentar luego.
    pub async fn get_or_detect<F, Fut, E>(&self, detect: F) -> MemoryBudget
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<HardwareInfo, E>>,
    {
        let detected = self
            .budget
            .get_or_try_init(|| async {
                let hardware = detect().await?;
                Ok::<_, E>(MemoryBudget::from_hardware(&hardware))
            })
            .await;
        detected
            .copied()
            .unwrap_or_else(|_| MemoryBudget::unknown())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Arch, GpuInfo, Os};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const GPU_BYTES: u64 = 10_302_888_345;

    fn rtx_4070_32gib() -> HardwareInfo {
        HardwareInfo {
            os: Os::Windows,
            arch: Arch::X86_64,
            avx2: true,
            avx512: false,
            gpus: vec![GpuInfo {
                name: "GPU".to_owned(),
                vram_mib: Some(12282),
            }],
            driver_cuda: None,
            vulkan_available: false,
            metal: false,
            ram_bytes: Some(34_359_738_368),
        }
    }

    #[tokio::test]
    async fn budget_cache_detects_once() {
        let cache = BudgetCache::new();
        let calls = AtomicUsize::new(0);
        let detect = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(rtx_4070_32gib())
        };
        let first = cache.get_or_detect(detect).await;
        let second = cache.get_or_detect(detect).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(first.gpu_bytes, Some(GPU_BYTES));
        assert_eq!(second.gpu_bytes, Some(GPU_BYTES));
    }

    #[tokio::test]
    async fn budget_cache_error_not_cached() {
        let cache = BudgetCache::new();
        let calls = AtomicUsize::new(0);
        let first = cache
            .get_or_detect(|| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<HardwareInfo, ()>(())
            })
            .await;
        let second = cache
            .get_or_detect(|| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(rtx_4070_32gib())
            })
            .await;
        assert_eq!(first, MemoryBudget::unknown());
        assert_eq!(second.gpu_bytes, Some(GPU_BYTES));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn budget_cache_concurrent_detects_once() {
        let cache = BudgetCache::new();
        let calls = AtomicUsize::new(0);
        let detect = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok::<_, ()>(rtx_4070_32gib())
        };
        let (first, second) =
            tokio::join!(cache.get_or_detect(detect), cache.get_or_detect(detect));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(first, second);
        assert_eq!(first.gpu_bytes, Some(GPU_BYTES));
    }
}

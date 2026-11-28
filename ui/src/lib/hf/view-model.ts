import type { Fit, FitHint, MemoryBudget, MemoryEstimate } from "./types";

const LOCALE = "es-ES";
const BYTES_PER_GIB = 1024 ** 3;
const THOUSAND = 1_000;
const MILLION = 1_000_000;

const oneDecimal = new Intl.NumberFormat(LOCALE, {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
});
const compact = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: 1 });
const utcDate = new Intl.DateTimeFormat(LOCALE, {
  timeZone: "UTC",
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
});

const FIT_LABELS: Record<Fit, string> = {
  Gpu: "Cabe en GPU",
  Ram: "Cabe en RAM",
  NoFit: "No cabe",
};

export function formatGiB(bytes: number): string {
  return `${oneDecimal.format(bytes / BYTES_PER_GIB)} GiB`;
}

export function formatCount(count: number): string {
  if (count >= MILLION) return `${compact.format(count / MILLION)} M`;
  if (count >= THOUSAND) return `${compact.format(count / THOUSAND)} mil`;
  return compact.format(count);
}

export function formatDate(iso: string): string {
  return utcDate.format(new Date(iso));
}

export function estimateLabel(estimate: MemoryEstimate): string {
  const qualifier = estimate.kv_included ? "orientativa" : "sin caché KV";
  return `≈ ${formatGiB(estimate.bytes)} (${qualifier})`;
}

export function fitLabel(fit: Fit | null): string | null {
  return fit === null ? null : FIT_LABELS[fit];
}

export function fitHintBadge(hint: FitHint | null): string | null {
  const label = hint === null ? null : fitLabel(hint.fit);
  return label === null ? null : `≈ ${label}`;
}

export function fitHintTitle(hint: FitHint): string {
  return `Orientativa: estimada para una cuantización Q4_K_M típica (≈ ${formatGiB(hint.estimate.bytes)})`;
}

export function budgetLegend(budget: MemoryBudget): string {
  const { gpu_bytes, ram_bytes, unified, usable_percent } = budget;
  const share = `${usable_percent} % del total`;
  if (unified && gpu_bytes !== null) {
    return `Memoria unificada: comparado con ${formatGiB(gpu_bytes)} utilizables (${share})`;
  }
  if (gpu_bytes !== null && ram_bytes !== null) {
    return `Comparado con ${formatGiB(gpu_bytes)} de VRAM y ${formatGiB(ram_bytes)} de RAM utilizables (${share})`;
  }
  if (ram_bytes !== null) {
    return `VRAM desconocida: solo se compara con la RAM (${formatGiB(ram_bytes)} utilizables, ${share})`;
  }
  if (gpu_bytes !== null) {
    return `Comparado con ${formatGiB(gpu_bytes)} de VRAM utilizables (${share}); RAM desconocida`;
  }
  return "No se pudo detectar la memoria del equipo";
}

import { describe, expect, it } from "vitest";
import type { FitHint, MemoryBudget } from "./types";
import {
  budgetLegend,
  estimateLabel,
  fitHintBadge,
  fitHintTitle,
  fitLabel,
  isValidTokenFormat,
  tokenStatusLabel,
  tokenSaveErrorMessage,
  formatCount,
  formatDate,
  formatGiB,
} from "./view-model";

const HINT_8B: FitHint = { estimate: { bytes: 6_481_006_131, kv_included: true }, fit: "Gpu" };

describe("formatGiB", () => {
  it("format_gib_one_decimal", () => {
    expect(formatGiB(5_993_741_824)).toBe("5,6 GiB");
  });

  it("format_gib_ctx_8192_example", () => {
    expect(formatGiB(6_530_612_736)).toBe("6,1 GiB");
  });

  it("format_gib_zero", () => {
    expect(formatGiB(0)).toBe("0,0 GiB");
  });

  it("format_gib_large", () => {
    expect(formatGiB(54_657_735_616)).toBe("50,9 GiB");
  });
});

describe("formatCount", () => {
  it("format_count_millions", () => {
    expect(formatCount(8_181_570)).toBe("8,2 M");
  });

  it("format_count_thousands", () => {
    expect(formatCount(12_345)).toBe("12,3 mil");
  });

  it("format_count_small", () => {
    expect(formatCount(999)).toBe("999");
  });
});

describe("formatDate", () => {
  it("format_date_utc", () => {
    expect(formatDate("2026-11-12T02:15:52.000Z")).toBe("12/11/2026");
  });
});

describe("estimateLabel", () => {
  it("estimate_label_with_kv", () => {
    expect(estimateLabel({ bytes: 6_530_612_736, kv_included: true })).toBe(
      "≈ 6,1 GiB (orientativa)",
    );
  });

  it("estimate_label_without_kv", () => {
    expect(estimateLabel({ bytes: 5_456_870_912, kv_included: false })).toBe(
      "≈ 5,1 GiB (sin caché KV)",
    );
  });
});

describe("fit", () => {
  it("fit_label_values", () => {
    expect(fitLabel("Gpu")).toBe("Cabe en GPU");
    expect(fitLabel("Ram")).toBe("Cabe en RAM");
    expect(fitLabel("NoFit")).toBe("No cabe");
    expect(fitLabel(null)).toBeNull();
  });

  it("fit_hint_badge_with_fit", () => {
    expect(fitHintBadge(HINT_8B)).toBe("≈ Cabe en GPU");
  });

  it("fit_hint_badge_without_fit", () => {
    expect(fitHintBadge({ estimate: HINT_8B.estimate, fit: null })).toBeNull();
    expect(fitHintBadge(null)).toBeNull();
  });

  it("fit_hint_title_text", () => {
    expect(fitHintTitle(HINT_8B)).toBe(
      "Orientativa: estimada para una cuantización Q4_K_M típica (≈ 6,0 GiB)",
    );
  });
});

describe("budgetLegend", () => {
  const budget = (partial: Partial<MemoryBudget>): MemoryBudget => ({
    gpu_bytes: null,
    ram_bytes: null,
    unified: false,
    usable_percent: 80,
    ...partial,
  });

  it("budget_legend_dedicated", () => {
    expect(budgetLegend(budget({ gpu_bytes: 20_605_776_691, ram_bytes: 27_487_790_694 }))).toBe(
      "Comparado con 19,2 GiB de VRAM y 25,6 GiB de RAM utilizables (80 % del total)",
    );
  });

  it("budget_legend_unified", () => {
    expect(
      budgetLegend(budget({ gpu_bytes: 13_743_895_347, ram_bytes: 13_743_895_347, unified: true })),
    ).toBe("Memoria unificada: comparado con 12,8 GiB utilizables (80 % del total)");
  });

  it("budget_legend_ram_only", () => {
    expect(budgetLegend(budget({ ram_bytes: 27_487_790_694 }))).toBe(
      "VRAM desconocida: solo se compara con la RAM (25,6 GiB utilizables, 80 % del total)",
    );
  });

  it("budget_legend_gpu_only", () => {
    expect(budgetLegend(budget({ gpu_bytes: 10_302_888_345 }))).toBe(
      "Comparado con 9,6 GiB de VRAM utilizables (80 % del total); RAM desconocida",
    );
  });

  it("budget_legend_unknown", () => {
    expect(budgetLegend(budget({}))).toBe("No se pudo detectar la memoria del equipo");
  });
});

describe("token", () => {
  const valid = `hf_${"a1B2".repeat(8).slice(0, 34)}`;

  it("token_format_valid", () => {
    expect(isValidTokenFormat(`  ${valid} `)).toBe(true);
  });

  it("token_format_invalid", () => {
    const spaced = `hf_${"a".repeat(15)} ${"b".repeat(19)}`;
    for (const token of ["abc", "hf_abc", spaced, ""]) {
      expect(isValidTokenFormat(token)).toBe(false);
    }
  });

  it("token_status_label_cases", () => {
    expect(tokenStatusLabel(null)).toBe("Sin token");
    expect(tokenStatusLabel({ present: false, user: null, invalid: false })).toBe("Sin token");
    expect(tokenStatusLabel({ present: true, user: "ana", invalid: false })).toBe("Conectado como ana");
    expect(tokenStatusLabel({ present: true, user: null, invalid: true })).toBe("Token no válido");
    expect(tokenStatusLabel({ present: true, user: null, invalid: false })).toBe(
      "Token guardado (sin verificar)",
    );
  });

  it("token_save_error_message_cases", () => {
    expect(tokenSaveErrorMessage({ code: "Unauthorized", message: "Acceso denegado" })).toBe("Token no válido");
    expect(tokenSaveErrorMessage({ code: "Secret", message: "Llavero" })).toBe("Llavero");
  });
});


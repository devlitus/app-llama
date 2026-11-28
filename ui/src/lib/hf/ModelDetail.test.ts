import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import ModelDetail from "./ModelDetail.svelte";
import type { DownloadRequest, Fit, Gated, MemoryBudget, MemoryEstimate, ModelDetail as Detail, VariantView } from "./types";

afterEach(clearMocks);

const REVISION = "9217a1b2c3d4e5f60718293a4b5c6d7e8f901234";

const ESTIMATE: MemoryEstimate = { bytes: 6_530_612_736, kv_included: true };

const GPU_AND_RAM: MemoryBudget = {
  gpu_bytes: 20_615_843_021,
  ram_bytes: 27_487_790_694,
  unified: false,
  usable_percent: 80,
};

const UNKNOWN_BUDGET: MemoryBudget = { gpu_bytes: null, ram_bytes: null, unified: false, usable_percent: 80 };

interface VariantOptions {
  quant?: string | null;
  fit?: Fit | null;
  recommended?: boolean;
  complete?: boolean;
  partCount?: number;
  estimate?: MemoryEstimate;
}

function variant(name: string, options: VariantOptions = {}): VariantView {
  const { quant = name, fit = "Gpu", recommended = false, complete = true, partCount = 1, estimate = ESTIMATE } = options;
  const parts = Array.from({ length: partCount }, (_, index) => ({
    path: `${name}-${index}.gguf`,
    size: 2_147_483_648,
    sha256: null,
  }));
  return {
    variant: { name, quant, parts, total_bytes: 2_147_483_648 * partCount, complete },
    estimate,
    fit,
    recommended,
  };
}

function detail(variants: VariantView[], overrides: Partial<Detail> = {}, gated: Gated = "Open"): Detail {
  return {
    info: {
      id: "acme/model-GGUF",
      gated,
      downloads: 10,
      likes: 1,
      gguf: { total: null, architecture: "llama", context_length: 32_768 },
      license: "apache-2.0",
      sha: null,
    },
    ctx: 8192,
    budget: GPU_AND_RAM,
    variants,
    revision: REVISION,
    ...overrides,
  };
}

function respondWith(result: Detail): void {
  mockIPC(() => result);
}

async function renderDetail(result: Detail): Promise<void> {
  respondWith(result);
  render(ModelDetail, { repoId: "acme/model-GGUF" });
  await screen.findByRole("table");
}

function dataRows(): HTMLElement[] {
  return screen.getAllByRole("row").slice(1);
}

describe("ModelDetail", () => {
  it("ficha_renders_variants", async () => {
    await renderDetail(detail([variant("Q4_K_M"), variant("Q5_K_M"), variant("Q8_0")]));

    const rows = dataRows();
    expect(rows).toHaveLength(3);
    expect(within(rows[0]).getByText("Q4_K_M")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Q5_K_M")).toBeInTheDocument();
    expect(within(rows[2]).getByText("Q8_0")).toBeInTheDocument();
    expect(within(rows[0]).getByText("2,0 GiB")).toBeInTheDocument();
  });

  it("ficha_shows_loading_then_header", async () => {
    respondWith(detail([variant("Q4_K_M")]));
    render(ModelDetail, { repoId: "acme/model-GGUF" });

    expect(screen.getByText("Cargando…")).toBeInTheDocument();
    await screen.findByRole("table");
    expect(screen.queryByText("Cargando…")).toBeNull();
    expect(screen.getByText("acme/model-GGUF")).toBeInTheDocument();
    expect(screen.getByText(/apache-2.0/)).toBeInTheDocument();
    expect(screen.getByText(/llama/)).toBeInTheDocument();
    expect(screen.getByText(/32768/)).toBeInTheDocument();
  });

  it("ficha_unknown_quant_label", async () => {
    await renderDetail(detail([variant("weird", { quant: null })]));

    expect(within(dataRows()[0]).getByText("desconocida")).toBeInTheDocument();
  });

  it("ficha_split_shows_parts", async () => {
    await renderDetail(detail([variant("BF16", { partCount: 2 })]));

    expect(within(dataRows()[0]).getByText("2 partes")).toBeInTheDocument();
  });

  it("ficha_incomplete_variant", async () => {
    await renderDetail(detail([variant("Q4_0", { complete: false })]));

    const row = dataRows()[0];
    expect(within(row).getByText("incompleta")).toBeInTheDocument();
    expect(within(row).queryByRole("button", { name: "Descargar" })).toBeNull();
  });

  it("ficha_estimate_orientative", async () => {
    await renderDetail(detail([variant("Q4_K_M", { estimate: ESTIMATE })]));

    expect(within(dataRows()[0]).getByText("≈ 6,1 GiB (orientativa)")).toBeInTheDocument();
    expect(screen.getByText(/con contexto de 8192 tokens/)).toBeInTheDocument();
  });

  it("ficha_estimate_fallback_ctx", async () => {
    const info = { ...detail([]).info, gguf: null };
    const estimate = { bytes: 5_456_870_912, kv_included: false };
    await renderDetail(detail([variant("Q4_K_M", { estimate })], { ctx: 4096, info }));

    expect(within(dataRows()[0]).getByText("≈ 5,1 GiB (sin caché KV)")).toBeInTheDocument();
    expect(screen.getByText(/con contexto de 4096 tokens/)).toBeInTheDocument();
  });

  it("variant_row_descargar_habilitado", async () => {
    await renderDetail(detail([variant("Q4_K_M")]));

    const button = within(dataRows()[0]).getByRole("button", { name: "Descargar" });
    expect(button).toBeEnabled();
  });

  it("variant_row_envia_revision_de_la_ficha", async () => {
    const requests: DownloadRequest[] = [];
    respondWith(detail([variant("BF16", { partCount: 2 })]));
    render(ModelDetail, { repoId: "acme/model-GGUF", onDownload: (request) => requests.push(request) });
    await screen.findByRole("table");

    await fireEvent.click(within(dataRows()[0]).getByRole("button", { name: "Descargar" }));

    expect(requests).toEqual([
      {
        repo_id: "acme/model-GGUF",
        revision: REVISION,
        parts: [
          { path: "BF16-0.gguf", size: 2_147_483_648, sha256: null },
          { path: "BF16-1.gguf", size: 2_147_483_648, sha256: null },
        ],
      },
    ]);
  });

  it("ficha_fit_labels", async () => {
    await renderDetail(
      detail([
        variant("Q2_K", { fit: "Gpu" }),
        variant("Q4_0", { fit: "Gpu" }),
        variant("Q6_K", { fit: "Ram" }),
        variant("Q8_0", { fit: "NoFit" }),
      ]),
    );

    const rows = dataRows();
    expect(within(rows[0]).getByText("Cabe en GPU")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Cabe en GPU")).toBeInTheDocument();
    expect(within(rows[2]).getByText("Cabe en RAM")).toBeInTheDocument();
    expect(within(rows[3]).getByText("No cabe")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Comparado con 19,2 GiB de VRAM y 25,6 GiB de RAM utilizables (80 % del total)",
      ),
    ).toBeInTheDocument();
  });

  it("ficha_recommended_highlight", async () => {
    await renderDetail(
      detail([
        variant("Q2_K", { fit: "Gpu" }),
        variant("model-Q4_0", { quant: "Q4_0", fit: "Gpu", recommended: true }),
        variant("Q6_K", { fit: "Ram" }),
        variant("Q8_0", { fit: "NoFit" }),
      ]),
    );

    expect(screen.getAllByText("Recomendada")).toHaveLength(1);
    const row = screen.getByLabelText("Recomendada para este equipo");
    expect(within(row).getByText("Q4_0")).toBeInTheDocument();
    expect(within(row).getByText("Recomendada")).toBeInTheDocument();
  });

  it("ficha_no_recommended", async () => {
    await renderDetail(detail([variant("Q6_K", { fit: "NoFit" }), variant("Q8_0", { fit: "NoFit" })]));

    expect(screen.queryByText("Recomendada")).toBeNull();
    expect(screen.getByText("Ninguna cuantización cabe en la memoria de este equipo")).toBeInTheDocument();
  });

  it("ficha_incomplete_no_fit_label", async () => {
    await renderDetail(detail([variant("Q4_0", { complete: false, fit: null })]));

    const row = dataRows()[0];
    expect(within(row).queryByText(/Cabe en/)).toBeNull();
    expect(within(row).queryByText("No cabe")).toBeNull();
    expect(within(row).queryByText("Recomendada")).toBeNull();
  });

  it("ficha_budget_unknown", async () => {
    await renderDetail(
      detail([variant("Q4_K_M", { fit: null }), variant("Q8_0", { fit: null })], { budget: UNKNOWN_BUDGET }),
    );

    expect(screen.getByText("No se pudo detectar la memoria del equipo")).toBeInTheDocument();
    expect(screen.queryByText(/Cabe en/)).toBeNull();
    expect(screen.queryByText("No cabe")).toBeNull();
    expect(screen.queryByText(/Ninguna cuantización cabe/)).toBeNull();
  });

  it("ficha_gated_notice", async () => {
    await renderDetail(detail([variant("Q4_K_M")], {}, "Manual"));

    expect(screen.getByText("Restringido")).toBeInTheDocument();
    expect(screen.getByText(/token de Hugging Face \(en Ajustes\)/)).toBeInTheDocument();
    expect(screen.getByText(/aceptar las condiciones en la web del modelo/)).toBeInTheDocument();
  });

  it("ficha_duplicate_variant_names", async () => {
    const first = variant("x");
    const second = variant("x");
    second.variant.parts = [{ path: "other/x.gguf", size: 2_147_483_648, sha256: null }];

    await renderDetail(detail([first, second]));

    expect(dataRows()).toHaveLength(2);
  });

  it("ficha_error_not_found", async () => {
    mockIPC(() => {
      throw { code: "NotFound", message: "Modelo no encontrado" };
    });
    render(ModelDetail, { repoId: "acme/missing" });

    expect(await screen.findByText("Modelo no encontrado")).toBeInTheDocument();
  });
});

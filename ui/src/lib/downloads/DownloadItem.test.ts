import { fireEvent, render, screen } from "@testing-library/svelte";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import DownloadItem from "./DownloadItem.svelte";
import type { UnitView } from "./store.svelte";
import type { FileStatus, UnitState } from "./types";

afterEach(clearMocks);

const noop = (): void => {};
const TOTAL = 4_831_838_208;

function unit(state: UnitState, overrides: Partial<UnitView> = {}): UnitView {
  return {
    id: 1,
    version: 1,
    repo_id: "ana/modelo-GGUF",
    revision: "9217aaaa",
    display_name: "modelo-Q4_K_M",
    state,
    total_bytes: TOTAL,
    files: [],
    ...overrides,
  };
}

function file(relative_path: string, size: number, status: FileStatus) {
  return { relative_path, size, status };
}

function bars(): HTMLProgressElement[] {
  return screen.queryAllByRole("progressbar") as HTMLProgressElement[];
}

describe("DownloadItem", () => {
  it("item_descargando", () => {
    render(DownloadItem, {
      onRemove: noop,
      unit: unit("Downloading", {
        progress: {
          phase: "Download",
          download: {
            downloaded_bytes: 1_288_490_189,
            total_bytes: TOTAL,
            bytes_per_sec: 12_897_485,
            eta_secs: 200,
          },
        },
      }),
    });

    expect(screen.getByText("modelo-Q4_K_M")).toBeInTheDocument();
    expect(screen.getByText("1,2 GiB de 4,5 GiB")).toBeInTheDocument();
    expect(screen.getByText("12,3 MiB/s")).toBeInTheDocument();
    expect(screen.getByText("3 min 20 s")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Pausar" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancelar" })).toBeInTheDocument();
  });

  it("item_verificando_no_retrocede", () => {
    render(DownloadItem, {
      onRemove: noop,
      unit: unit("Downloading", {
        progress: {
          phase: "Verify",
          download: {
            downloaded_bytes: TOTAL,
            total_bytes: TOTAL,
            bytes_per_sec: 0,
            eta_secs: null,
          },
          verify: {
            downloaded_bytes: 0,
            total_bytes: TOTAL,
            bytes_per_sec: 1_610_612_736,
            eta_secs: 3,
          },
        },
      }),
    });

    expect(bars()).toHaveLength(2);
    const [main, verify] = bars();
    expect(main.value).toBe(TOTAL);
    expect(screen.getByText("Verificando")).toBeInTheDocument();
    expect(verify.value).toBe(0);
    expect(screen.getByText("Verificando… 1,5 GiB/s")).toBeInTheDocument();
    expect(screen.getByText("3 s")).toBeInTheDocument();
  });

  it("item_verificando_sin_evento_verify", () => {
    render(DownloadItem, { onRemove: noop, unit: unit("Verifying") });

    expect(screen.getByText("Verificando…")).toBeInTheDocument();
    expect(screen.queryByText(/\/s/)).toBeNull();
  });

  it("item_sin_progreso_usa_snapshot", () => {
    render(DownloadItem, {
      onRemove: noop,
      unit: unit("Paused", {
        files: [
          file("a.gguf", 1_073_741_824, "Verified"),
          file("b.gguf", 3_758_096_384, "Pending"),
        ],
      }),
    });

    expect(bars()).toHaveLength(1);
    const [main] = bars();
    expect(main.max).toBe(TOTAL);
    expect(main.value).toBe(1_073_741_824);
    expect(screen.getByText("1,0 GiB de 4,5 GiB")).toBeInTheDocument();
    expect(screen.queryByText(/Verificando/)).toBeNull();
    expect(screen.queryByText(/\/s/)).toBeNull();
  });

  it("item_completada_sale_llena", () => {
    render(DownloadItem, {
      onRemove: noop,
      unit: unit("Completed", {
        files: [file("a.gguf", TOTAL / 2, "Verified"), file("b.gguf", TOTAL / 2, "Preexisting")],
      }),
    });

    expect(bars()[0].value).toBe(TOTAL);
  });

  it("item_fallida_muestra_el_motivo", () => {
    render(DownloadItem, { onRemove: noop, unit: unit({ Failed: "NotFound" }) });

    expect(screen.getByText("El fichero ya no existe en Hugging Face.")).toBeInTheDocument();
  });

  it("item_botones_por_estado", () => {
    const labels = () => screen.getAllByRole("button").map((button) => button.textContent?.trim());

    const paused = render(DownloadItem, { onRemove: noop, unit: unit("Paused") });
    expect(labels()).toEqual(["Reanudar", "Cancelar"]);
    paused.unmount();

    const failed = render(DownloadItem, { onRemove: noop, unit: unit({ Failed: "Network" }) });
    expect(labels()).toEqual(["Reintentar", "Quitar"]);
    failed.unmount();

    render(DownloadItem, { onRemove: noop, unit: unit("Completed") });
    expect(labels()).toEqual(["Quitar"]);
  });

  it("item_pulsar_pausar", async () => {
    const calls: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
    });
    render(DownloadItem, { onRemove: noop, unit: unit("Downloading") });

    await fireEvent.click(screen.getByRole("button", { name: "Pausar" }));

    expect(calls).toEqual([{ cmd: "downloads_pause", args: { id: 1 } }]);
  });

  it("item_quitar_delega_en_on_remove", async () => {
    const removed: number[] = [];
    render(DownloadItem, { unit: unit("Completed"), onRemove: (id: number) => removed.push(id) });

    await fireEvent.click(screen.getByRole("button", { name: "Quitar" }));

    expect(removed).toEqual([1]);
  });

  it("item_error_de_orden_se_muestra", async () => {
    mockIPC(() => {
      throw { code: "UnknownDownload", message: "Descarga desconocida" };
    });
    render(DownloadItem, { onRemove: noop, unit: unit("Downloading") });

    await fireEvent.click(screen.getByRole("button", { name: "Pausar" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Descarga desconocida");
  });

  it("item_progress_sin_style", () => {
    render(DownloadItem, {
      onRemove: noop,
      unit: unit("Downloading", {
        progress: {
          phase: "Verify",
          download: { downloaded_bytes: TOTAL, total_bytes: TOTAL, bytes_per_sec: 0, eta_secs: null },
          verify: { downloaded_bytes: 1, total_bytes: TOTAL, bytes_per_sec: 1, eta_secs: 1 },
        },
      }),
    });

    expect(bars()).toHaveLength(2);
    for (const bar of bars()) {
      expect(bar).not.toHaveAttribute("style");
    }
  });
});

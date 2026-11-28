import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import type { ErrorPayload } from "../errors";
import {
  actionsFor,
  applyProgress,
  applyState,
  DownloadsStore,
  mergeList,
  removeUnit,
  type UnitView,
} from "./store.svelte";
import type { DownloadProgress, DownloadSnapshot, ProgressPhase, UnitState } from "./types";

afterEach(clearMocks);

function snapshot(id: number, version: number, state: UnitState = "Queued"): DownloadSnapshot {
  return {
    id,
    version,
    repo_id: "ana/modelo-GGUF",
    revision: "main",
    display_name: `modelo-${id}`,
    state,
    total_bytes: 2097152,
    files: [],
  };
}

function view(id: number, version: number, state: UnitState = "Queued"): UnitView {
  return snapshot(id, version, state);
}

function progress(id: number, phase: ProgressPhase, downloaded: number): DownloadProgress {
  return {
    id,
    downloaded_bytes: downloaded,
    total_bytes: 2097152,
    bytes_per_sec: 1000,
    eta_secs: 5,
    phase,
  };
}

const ids = (units: readonly UnitView[]) => units.map((unit) => unit.id);

describe("applyState", () => {
  it("applyState_inserta_y_reemplaza", () => {
    const first = applyState([], snapshot(1, 1, "Queued"));
    const second = applyState(first, snapshot(1, 2, "Paused"));

    expect(second).toHaveLength(1);
    expect(second[0].state).toBe("Paused");
  });

  it("applyState_descarta_version_vieja", () => {
    const newer = applyState([], snapshot(1, 3, "Paused"));
    const result = applyState(newer, snapshot(1, 2, "Queued"));

    expect(result).toHaveLength(1);
    expect(result[0].version).toBe(3);
    expect(result[0].state).toBe("Paused");
  });

  it("applyState_conserva_el_progreso", () => {
    const withProgress = applyProgress([view(1, 1, "Downloading")], progress(1, "Download", 100));
    const result = applyState(withProgress, snapshot(1, 2, "Verifying"));

    expect(result[0].progress?.download?.downloaded_bytes).toBe(100);
  });

  it("applyState_limpia_progreso_al_salir_de_ejecucion", () => {
    let first = applyProgress([view(1, 1, "Downloading")], progress(1, "Verify", 5));
    first = applyState(first, snapshot(1, 2, { Failed: "ChecksumMismatch" }));
    expect(first[0].progress).toBeUndefined();
    first = applyState(first, snapshot(1, 3, "Queued"));
    expect(first[0].progress).toBeUndefined();

    const second = applyProgress([view(2, 1, "Downloading")], progress(2, "Verify", 5));
    expect(second[0].progress?.phase).toBe("Verify");
    expect(applyState(second, snapshot(2, 2, "Paused"))[0].progress).toBeUndefined();
  });
});

describe("mergeList", () => {
  it("mergeList_no_pisa_evento_nuevo", () => {
    const result = mergeList([view(1, 5)], [snapshot(1, 4), snapshot(2, 1)], new Set([1]));

    expect(ids(result)).toEqual([1, 2]);
    expect(result[0].version).toBe(5);
  });

  it("mergeList_quita_ausentes", () => {
    const result = mergeList([view(1, 1), view(2, 1)], [snapshot(2, 1)], new Set([1, 2]));

    expect(ids(result)).toEqual([2]);
  });

  it("mergeList_lista_vacia_quita_conocidas", () => {
    expect(mergeList([view(1, 1)], [], new Set([1]))).toEqual([]);
  });

  it("mergeList_conserva_llegadas_por_evento", () => {
    const result = mergeList([view(1, 1), view(3, 1)], [snapshot(1, 1)], new Set([1]));

    expect(ids(result)).toEqual([1, 3]);
  });
});

describe("mergeList y progreso", () => {
  it("mergeList_limpia_progreso_al_salir_de_ejecucion", () => {
    const running = applyProgress([view(1, 1, "Downloading")], progress(1, "Verify", 5));

    const result = mergeList(running, [snapshot(1, 2, "Paused")], new Set([1]));

    expect(result[0].state).toBe("Paused");
    expect(result[0].progress).toBeUndefined();
  });
});

describe("removeUnit", () => {
  it("removeUnit_quita_por_id", () => {
    const units = [view(1, 1), view(2, 1)];
    const afterRemoval = removeUnit(units, 1);

    expect(ids(afterRemoval)).toEqual([2]);
    expect(removeUnit(afterRemoval, 9)).toEqual(afterRemoval);
  });
});

describe("applyProgress", () => {
  it("applyProgress_actualiza_unidad", () => {
    const result = applyProgress([view(1, 1, "Downloading")], progress(1, "Download", 1048576));

    expect(result[0].progress?.download).toEqual({
      downloaded_bytes: 1048576,
      total_bytes: 2097152,
      bytes_per_sec: 1000,
      eta_secs: 5,
    });
    expect(result[0].progress?.phase).toBe("Download");
  });

  it("applyProgress_por_fase", () => {
    let units = [view(1, 1, "Downloading")];
    units = applyProgress(units, progress(1, "Download", 1048576));
    units = applyProgress(units, progress(1, "Verify", 524288));

    expect(units[0].progress?.download?.downloaded_bytes).toBe(1048576);
    expect(units[0].progress?.verify?.downloaded_bytes).toBe(524288);

    units = applyProgress(units, progress(1, "Download", 1572864));

    expect(units[0].progress?.download?.downloaded_bytes).toBe(1572864);
    expect(units[0].progress?.verify?.downloaded_bytes).toBe(524288);
    expect(units[0].progress?.phase).toBe("Download");
  });

  it("applyProgress_en_verificacion", () => {
    const result = applyProgress([view(1, 1, "Verifying")], progress(1, "Verify", 10));

    expect(result[0].progress?.verify?.downloaded_bytes).toBe(10);
  });

  it("applyProgress_id_desconocido", () => {
    const units = [view(1, 1, "Downloading")];

    expect(applyProgress(units, progress(9, "Download", 1))).toEqual(units);
  });

  it("applyProgress_tras_pausa_ignorado", () => {
    const units = [view(1, 1, "Paused")];

    expect(applyProgress(units, progress(1, "Download", 1))).toEqual(units);
  });
});

describe("actionsFor", () => {
  it("actions_por_estado", () => {
    const states: UnitState[] = [
      "Queued",
      "Downloading",
      "Verifying",
      "Paused",
      { Failed: "Network" },
      "Completed",
      "Cancelled",
    ];

    expect(states.map(actionsFor)).toEqual([
      ["pause", "cancel"],
      ["pause", "cancel"],
      ["cancel"],
      ["resume", "cancel"],
      ["retry", "remove"],
      ["remove"],
      ["remove"],
    ]);
  });
});

describe("DownloadsStore", () => {
  it("store_se_suscribe_a_eventos", async () => {
    mockIPC(() => [], { shouldMockEvents: true });
    const store = new DownloadsStore();

    await store.start();
    await emit("downloads://state", snapshot(1, 1, "Downloading"));
    expect(ids(store.units)).toEqual([1]);

    await emit("downloads://progress", progress(1, "Download", 7));
    expect(store.units[0].progress?.download?.downloaded_bytes).toBe(7);

    store.stop();
    await emit("downloads://state", snapshot(1, 2, "Paused"));
    expect(store.units[0].state).toBe("Downloading");
    store.stop();
  });

  it("start_y_stop_inmediato_no_deja_suscripciones", async () => {
    mockIPC(() => [], { shouldMockEvents: true });
    const store = new DownloadsStore();

    const started = store.start();
    store.stop();
    await started;
    await emit("downloads://state", snapshot(1, 1));

    expect(store.units).toEqual([]);
  });

  it("start_concurrente_no_pierde_suscripciones", async () => {
    mockIPC(() => [], { shouldMockEvents: true });
    const store = new DownloadsStore();

    await Promise.all([store.start(), store.start()]);
    store.stop();
    await emit("downloads://state", snapshot(1, 1));

    expect(store.units).toEqual([]);
  });

  it("quitar_desaparece_de_la_lista", async () => {
    let list = [snapshot(1, 1, "Completed"), snapshot(2, 1, "Paused")];
    const calls: Array<[string, unknown]> = [];
    mockIPC(
      (cmd, args) => {
        calls.push([cmd, args]);
        return cmd === "downloads_list" ? list : null;
      },
      { shouldMockEvents: true },
    );
    const store = new DownloadsStore();

    await store.start();
    await store.remove(1);

    expect(calls).toContainEqual(["downloads_remove", { id: 1 }]);
    expect(ids(store.units)).toEqual([2]);

    list = [snapshot(2, 1, "Paused")];
    store.stop();
    await store.start();

    expect(ids(store.units)).toEqual([2]);
    store.stop();
  });

  it("quitar_rechazado_conserva_unidad", async () => {
    const payload: ErrorPayload = { code: "Io", message: "no se pudo borrar" };
    const failed = snapshot(1, 1, { Failed: "Network" });
    mockIPC((cmd) => (cmd === "downloads_list" ? [failed] : Promise.reject(payload)), {
      shouldMockEvents: true,
    });
    const store = new DownloadsStore();
    await store.start();

    const failure = await store.remove(1).catch((error: unknown) => error);

    expect(failure).toBe(payload);
    expect(ids(store.units)).toEqual([1]);
    store.stop();
  });
});

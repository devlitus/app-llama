import type { UnlistenFn } from "@tauri-apps/api/event";
import { SvelteSet } from "svelte/reactivity";
import {
  downloadsList,
  downloadsRemove,
  onDownloadProgress,
  onDownloadState,
} from "./api";
import type {
  DownloadId,
  DownloadProgress,
  DownloadSnapshot,
  ProgressPhase,
  UnitState,
} from "./types";

export interface PhaseProgress {
  downloaded_bytes: number;
  total_bytes: number;
  bytes_per_sec: number;
  eta_secs: number | null;
}

// Cada fase tiene su propia barra: `downloaded_bytes` puede bajar al cambiar de fase.
export interface UnitProgress {
  download?: PhaseProgress;
  verify?: PhaseProgress;
  phase: ProgressPhase;
}

export interface UnitView extends DownloadSnapshot {
  progress?: UnitProgress;
}

export type UnitAction = "pause" | "resume" | "cancel" | "retry" | "remove";

const PHASE_KEY = { Download: "download", Verify: "verify" } as const;

function isRunning(state: UnitState): boolean {
  return state === "Downloading" || state === "Verifying";
}

// El progreso solo tiene sentido entre estados en ejecución; en otro caso la vista usa el snapshot.
function replaceUnit(previous: UnitView, snapshot: DownloadSnapshot): UnitView {
  return isRunning(previous.state) && isRunning(snapshot.state)
    ? { ...snapshot, progress: previous.progress }
    : snapshot;
}

export function applyState(units: readonly UnitView[], snapshot: DownloadSnapshot): UnitView[] {
  const known = units.find((unit) => unit.id === snapshot.id);
  if (!known) return [...units, snapshot];
  if (snapshot.version < known.version) return [...units];
  return units.map((unit) => (unit.id === snapshot.id ? replaceUnit(unit, snapshot) : unit));
}

function withPhase(progress: UnitProgress | undefined, event: DownloadProgress): UnitProgress {
  const phaseProgress: PhaseProgress = {
    downloaded_bytes: event.downloaded_bytes,
    total_bytes: event.total_bytes,
    bytes_per_sec: event.bytes_per_sec,
    eta_secs: event.eta_secs,
  };
  return { ...progress, [PHASE_KEY[event.phase]]: phaseProgress, phase: event.phase };
}

export function applyProgress(units: readonly UnitView[], event: DownloadProgress): UnitView[] {
  return units.map((unit) =>
    unit.id === event.id && isRunning(unit.state)
      ? { ...unit, progress: withPhase(unit.progress, event) }
      : unit,
  );
}

export function mergeList(
  units: readonly UnitView[],
  list: readonly DownloadSnapshot[],
  knownBefore: ReadonlySet<DownloadId>,
): UnitView[] {
  const kept = units.filter(
    (unit) => list.some((snapshot) => snapshot.id === unit.id) || !knownBefore.has(unit.id),
  );
  return list.reduce(applyState, kept);
}

export function removeUnit(units: readonly UnitView[], id: DownloadId): UnitView[] {
  return units.filter((unit) => unit.id !== id);
}

export function actionsFor(state: UnitState): UnitAction[] {
  if (typeof state === "object") return ["retry", "remove"];
  switch (state) {
    case "Queued":
    case "Downloading":
      return ["pause", "cancel"];
    case "Verifying":
      return ["cancel"];
    case "Paused":
      return ["resume", "cancel"];
    case "Completed":
    case "Cancelled":
      return ["remove"];
  }
}

export class DownloadsStore {
  units = $state<UnitView[]>([]);

  private unlisteners: UnlistenFn[] = [];
  private generation = 0;

  async start(): Promise<void> {
    this.stop();
    const generation = this.generation;
    const fresh = await Promise.all([
      onDownloadState((snapshot) => (this.units = applyState(this.units, snapshot))),
      onDownloadProgress((event) => (this.units = applyProgress(this.units, event))),
    ]);
    if (this.generation !== generation) {
      fresh.forEach((unlisten) => unlisten());
      return;
    }
    this.unlisteners = fresh;
    const knownBefore = new SvelteSet(this.units.map((unit) => unit.id));
    const list = await downloadsList();
    if (this.generation !== generation) return;
    this.units = mergeList(this.units, list, knownBefore);
  }

  stop(): void {
    this.generation += 1;
    this.unlisteners.forEach((unlisten) => unlisten());
    this.unlisteners = [];
  }

  async remove(id: DownloadId): Promise<void> {
    await downloadsRemove(id);
    this.units = removeUnit(this.units, id);
  }
}

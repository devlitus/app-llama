import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  ActiveRuntime,
  BackendChoice,
  HardwareInfo,
  ProgressEvent,
  RuntimeStatus,
  UpdateInfo,
} from "./types";
import type { FileFilter } from "./view-model";

const PROGRESS_EVENT = "runtime://progress";

export function runtimeHardware(): Promise<HardwareInfo> {
  return invoke<HardwareInfo>("runtime_hardware");
}

export function runtimeStatus(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("runtime_status");
}

export function runtimeCheckUpdate(): Promise<UpdateInfo> {
  return invoke<UpdateInfo>("runtime_check_update");
}

export function runtimeInstall(preference: BackendChoice): Promise<ActiveRuntime> {
  return invoke<ActiveRuntime>("runtime_install", { preference });
}

export function runtimeCancelInstall(): Promise<void> {
  return invoke<void>("runtime_cancel_install");
}

export function runtimeUseExternal(path: string): Promise<ActiveRuntime> {
  return invoke<ActiveRuntime>("runtime_use_external", { path });
}

export function onRuntimeProgress(handler: (event: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>(PROGRESS_EVENT, (event) => handler(event.payload));
}

/** `null` si el usuario cierra el diálogo sin elegir. */
export function pickExternalBinary(filters: FileFilter[] | undefined): Promise<string | null> {
  return open({ multiple: false, directory: false, filters });
}

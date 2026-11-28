import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DownloadRequest } from "../hf/types";
import type { DownloadId, DownloadProgress, DownloadSnapshot } from "./types";

const STATE_EVENT = "downloads://state";
const PROGRESS_EVENT = "downloads://progress";

export function downloadsEnqueueHf(request: DownloadRequest): Promise<DownloadId> {
  return invoke<DownloadId>("downloads_enqueue_hf", { request });
}

export function downloadsList(): Promise<DownloadSnapshot[]> {
  return invoke<DownloadSnapshot[]>("downloads_list");
}

export function downloadsPause(id: DownloadId): Promise<void> {
  return invoke<void>("downloads_pause", { id });
}

export function downloadsResume(id: DownloadId): Promise<void> {
  return invoke<void>("downloads_resume", { id });
}

export function downloadsCancel(id: DownloadId): Promise<void> {
  return invoke<void>("downloads_cancel", { id });
}

export function downloadsRetry(id: DownloadId): Promise<void> {
  return invoke<void>("downloads_retry", { id });
}

export function downloadsRemove(id: DownloadId): Promise<void> {
  return invoke<void>("downloads_remove", { id });
}

export function onDownloadState(handler: (snapshot: DownloadSnapshot) => void): Promise<UnlistenFn> {
  return listen<DownloadSnapshot>(STATE_EVENT, (event) => handler(event.payload));
}

export function onDownloadProgress(
  handler: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>(PROGRESS_EVENT, (event) => handler(event.payload));
}

// Espejo del JSON que fijan los tests `json_download_snapshot_para_la_ui` y
// `json_download_progress_para_la_ui` de `src-tauri/src/commands/downloads.rs`.
// Serde por defecto: las variantes con datos son `{ Variante: datos }`.

export type DownloadId = number;

export type FileStatus = "Pending" | "Verified" | "Preexisting";

export type FailureKind =
  | "Network"
  | "Unauthorized"
  | "Gated"
  | "NotFound"
  | "ChecksumMismatch"
  | "UnexpectedContentRange"
  | "DestinationExists"
  | { SizeMismatch: { expected: number; actual: number } }
  | { InsufficientSpace: { needed: number; available: number } }
  | { Io: { message: string } };

export type UnitState =
  | "Queued"
  | "Downloading"
  | "Verifying"
  | "Paused"
  | "Completed"
  | "Cancelled"
  | { Failed: FailureKind };

export interface FileSnapshot {
  relative_path: string;
  size: number;
  status: FileStatus;
}

export interface DownloadSnapshot {
  id: DownloadId;
  version: number;
  repo_id: string;
  revision: string;
  display_name: string;
  state: UnitState;
  total_bytes: number;
  files: FileSnapshot[];
}

export type ProgressPhase = "Download" | "Verify";

export interface DownloadProgress {
  id: DownloadId;
  downloaded_bytes: number;
  total_bytes: number;
  bytes_per_sec: number;
  eta_secs: number | null;
  phase: ProgressPhase;
}

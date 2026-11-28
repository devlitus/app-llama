// Espejo de la serialización serde de Rust (sin `rename_all`). La forma exacta está fijada por los
// tests `json_*_para_la_ui` de `src-tauri/src/commands/hf.rs`.

export type Gated = "Open" | "Auto" | "Manual";
export type SortBy = "downloads" | "likes";
export type Fit = "Gpu" | "Ram" | "NoFit";

export interface GgufInfo {
  total: number | null;
  architecture: string | null;
  context_length: number | null;
}

export interface ModelSummary {
  id: string;
  downloads: number;
  likes: number;
  gated: Gated;
  last_modified: string | null;
  pipeline_tag: string | null;
  gguf: GgufInfo | null;
}

export interface MemoryEstimate {
  bytes: number;
  kv_included: boolean;
}

export interface FitHint {
  estimate: MemoryEstimate;
  fit: Fit | null;
}

export interface SearchItem {
  model: ModelSummary;
  hint: FitHint | null;
}

export interface SearchPage {
  items: SearchItem[];
  next: string | null;
}

export interface MemoryBudget {
  gpu_bytes: number | null;
  ram_bytes: number | null;
  unified: boolean;
  usable_percent: number;
}

export interface GgufPart {
  path: string;
  size: number;
  sha256: string | null;
}

export interface GgufVariant {
  name: string;
  quant: string | null;
  parts: GgufPart[];
  total_bytes: number;
  complete: boolean;
}

export interface DownloadRequest {
  repo_id: string;
  revision: string;
  parts: GgufPart[];
}

export interface VariantView {
  variant: GgufVariant;
  estimate: MemoryEstimate;
  fit: Fit | null;
  recommended: boolean;
}

export interface ModelInfo {
  id: string;
  gated: Gated;
  downloads: number;
  likes: number;
  gguf: GgufInfo | null;
  license: string | null;
  sha: string | null;
}

export interface ModelDetail {
  info: ModelInfo;
  ctx: number;
  budget: MemoryBudget;
  variants: VariantView[];
  revision: string;
}

export interface TokenStatus {
  present: boolean;
  user: string | null;
  invalid: boolean;
}

export interface WhoAmI {
  name: string;
  role: string | null;
}

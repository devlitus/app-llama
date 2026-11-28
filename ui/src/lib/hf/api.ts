import { invoke } from "@tauri-apps/api/core";
import type { ModelDetail, SearchPage, SortBy, TokenStatus, WhoAmI } from "./types";

export function hfSearch(query: string, sort: SortBy): Promise<SearchPage> {
  return invoke<SearchPage>("hf_search", { query, sort });
}

export function hfSearchNext(nextUrl: string): Promise<SearchPage> {
  return invoke<SearchPage>("hf_search_next", { nextUrl });
}

export function hfModelDetail(repoId: string): Promise<ModelDetail> {
  return invoke<ModelDetail>("hf_model_detail", { repoId });
}

export function hfTokenStatus(): Promise<TokenStatus> {
  return invoke<TokenStatus>("hf_token_status");
}

export function hfTokenSave(token: string): Promise<WhoAmI> {
  return invoke<WhoAmI>("hf_token_save", { token });
}

export function hfTokenDelete(): Promise<void> {
  return invoke<void>("hf_token_delete");
}

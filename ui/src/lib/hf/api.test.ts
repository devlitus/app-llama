import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import type { ErrorPayload } from "../errors";
import { hfModelDetail, hfSearch, hfSearchNext, hfTokenSave } from "./api";

afterEach(clearMocks);

type Call = [string, unknown];

function recordCalls(response: unknown): Call[] {
  const calls: Call[] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return response;
  });
  return calls;
}

describe("api de Hugging Face", () => {
  it("api_hf_search_invokes_command", async () => {
    const page = { items: [], next: null };
    const calls = recordCalls(page);

    await expect(hfSearch("qwen", "downloads")).resolves.toEqual(page);
    expect(calls).toEqual([["hf_search", { query: "qwen", sort: "downloads" }]]);
  });

  it("api_hf_search_next_sends_next_url", async () => {
    const calls = recordCalls({ items: [], next: null });

    await hfSearchNext("https://huggingface.co/api/models?cursor=abc");
    expect(calls).toEqual([
      ["hf_search_next", { nextUrl: "https://huggingface.co/api/models?cursor=abc" }],
    ]);
  });

  it("api_hf_model_detail_sends_repo_id", async () => {
    const calls = recordCalls({});

    await hfModelDetail("a/b");
    expect(calls).toEqual([["hf_model_detail", { repoId: "a/b" }]]);
  });

  it("api_hf_token_save_sends_token", async () => {
    const calls = recordCalls({ name: "ana" });

    await hfTokenSave("hf_abc");
    expect(calls).toEqual([["hf_token_save", { token: "hf_abc" }]]);
  });

  it("api_propagates_rejection", async () => {
    const failure: ErrorPayload = { code: "NotFound", message: "Modelo no encontrado" };
    mockIPC(() => {
      throw failure;
    });

    await expect(hfModelDetail("a/b")).rejects.toBe(failure);
  });
});

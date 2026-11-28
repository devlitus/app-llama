import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import type { ErrorPayload } from "../errors";
import type { DownloadRequest } from "../hf/types";
import {
  downloadsCancel,
  downloadsEnqueueHf,
  downloadsList,
  downloadsPause,
  downloadsRemove,
  downloadsResume,
  downloadsRetry,
} from "./api";

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

const REQUEST: DownloadRequest = {
  repo_id: "ana/modelo-GGUF",
  revision: "9217f5d",
  parts: [{ path: "m.gguf", size: 40, sha256: null }],
};

describe("api de descargas", () => {
  it("api_enqueue_pasa_la_peticion", async () => {
    const calls = recordCalls(7);

    await expect(downloadsEnqueueHf(REQUEST)).resolves.toBe(7);
    expect(calls).toHaveLength(1);
    expect(calls[0][0]).toBe("downloads_enqueue_hf");
    const args = calls[0][1] as { request: DownloadRequest };
    expect(args.request).toBe(REQUEST);
  });

  it("api_enqueue_propaga_el_mismo_error", async () => {
    const payload: ErrorPayload = { code: "AlreadyQueued", message: "ya está en la cola" };
    mockIPC(() => Promise.reject(payload));

    const failure = await downloadsEnqueueHf(REQUEST).catch((error: unknown) => error);

    expect(failure).toBe(payload);
  });

  it("api_list_invoca_el_comando", async () => {
    const calls = recordCalls([]);

    await expect(downloadsList()).resolves.toEqual([]);
    expect(calls).toEqual([["downloads_list", {}]]);
  });

  it.each([
    ["downloads_pause", downloadsPause],
    ["downloads_resume", downloadsResume],
    ["downloads_cancel", downloadsCancel],
    ["downloads_retry", downloadsRetry],
    ["downloads_remove", downloadsRemove],
  ])("api_%s_envia_el_id", async (command, call) => {
    const calls = recordCalls(null);

    await call(4);

    expect(calls).toEqual([[command, { id: 4 }]]);
  });
});

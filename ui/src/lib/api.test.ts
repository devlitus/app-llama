import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { getAppInfo } from "./api";

afterEach(clearMocks);

describe("getAppInfo", () => {
  it("get_app_info_invoca_app_info", async () => {
    const commands: string[] = [];
    mockIPC((cmd) => {
      commands.push(cmd);
      return { name: "app-llama", version: "0.1.0" };
    });

    await expect(getAppInfo()).resolves.toEqual({ name: "app-llama", version: "0.1.0" });
    expect(commands).toEqual(["app_info"]);
  });

  it("get_app_info_propaga_error", async () => {
    mockIPC(() => {
      throw new Error("ipc");
    });

    await expect(getAppInfo()).rejects.toBeDefined();
  });
});

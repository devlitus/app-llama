import { describe, expect, it } from "vitest";
import { failureMessage, formatBytes, formatEta, formatSpeed } from "./format";
import type { FailureKind } from "./types";

describe("formato de descargas", () => {
  it("formatBytes_unidades", () => {
    expect(
      [0, 1023, 1536, 4831838208, 64424509440].map(formatBytes),
    ).toEqual(["0 B", "1023 B", "1,5 KiB", "4,5 GiB", "60,0 GiB"]);
  });

  it("formatBytes_mib", () => {
    expect(formatBytes(5 * 1024 * 1024)).toBe("5,0 MiB");
  });

  it("formatSpeed_mib_s", () => {
    expect(formatSpeed(12897485)).toBe("12,3 MiB/s");
  });

  it("formatEta_casos", () => {
    expect([null, 0, 59, 200, 7322].map(formatEta)).toEqual([
      "—",
      "0 s",
      "59 s",
      "3 min 20 s",
      "2 h 2 min",
    ]);
  });

  it("formatEta_omite_restos_en_cero", () => {
    expect([180, 7200].map(formatEta)).toEqual(["3 min", "2 h"]);
  });

  it("failureMessage_espacio", () => {
    const message = failureMessage({
      InsufficientSpace: { needed: 65498251264, available: 42949672960 },
    });

    expect(message).toContain("61,0 GiB");
    expect(message).toContain("40,0 GiB");
  });

  it("failureMessage_por_tipo", () => {
    const kinds: FailureKind[] = ["Gated", "Unauthorized", "Network", "ChecksumMismatch"];
    const messages = kinds.map(failureMessage);

    expect(messages.every((message) => message.length > 0)).toBe(true);
    expect(new Set(messages).size).toBe(4);
    expect(failureMessage("Gated")).toContain("condiciones");
    expect(failureMessage("Gated")).toContain("web del modelo");
  });

  it("failureMessage_variantes_con_datos", () => {
    expect(failureMessage({ Io: { message: "disco lleno" } })).toContain("disco lleno");
    expect(failureMessage({ SizeMismatch: { expected: 10, actual: 12 } })).toContain("10");
    expect(failureMessage("NotFound")).not.toBe("");
    expect(failureMessage("UnexpectedContentRange")).not.toBe("");
    expect(failureMessage("DestinationExists")).not.toBe("");
  });
});

import { describe, expect, it } from "vitest";
import { errorMessage } from "./errors";

describe("errorMessage", () => {
  it("mensaje_error_rate_limit", () => {
    const message = errorMessage({
      code: "RateLimited",
      message: "Se alcanzó el límite de GitHub. Inténtalo más tarde.",
    });

    expect(message).toContain("Inténtalo más tarde");
  });

  it("mensaje_error_desconocido", () => {
    expect(errorMessage(new Error("ipc"))).toBe("Ha ocurrido un error inesperado.");
    expect(errorMessage(undefined)).toBe("Ha ocurrido un error inesperado.");
  });
});

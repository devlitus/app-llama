import { test } from "node:test";
import assert from "node:assert/strict";
import { MemoryInputError, PreconditionError, UsageError } from "./memory-errors.mjs";
import { MemoryInputError as ReexportedInputError } from "./memory-entry.mjs";

test("errores_codigos_de_salida", () => {
  assert.equal(new UsageError("x").code, 1);
  assert.equal(new MemoryInputError("x").code, 2);
  assert.equal(new PreconditionError("x").code, 3);
});

test("errores_entrada_se_reexporta_desde_entry", () => {
  assert.equal(ReexportedInputError, MemoryInputError);
});

test("errores_precondicion_conserva_la_causa", () => {
  const cause = new Error("raíz");
  assert.equal(new PreconditionError("x", { cause }).cause, cause);
  assert.equal(new PreconditionError("x").cause, undefined);
});

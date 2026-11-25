import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { writeFileAtomic } from "./memory-fs.mjs";
import { PreconditionError } from "./memory-errors.mjs";

function withDir(body) {
  const dir = mkdtempSync(join(tmpdir(), "mem-"));
  try {
    body(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("fs_escritura_atomica_crea_directorios_y_no_deja_temporales", () =>
  withDir((dir) => {
    const path = join(dir, "a", "b", "doc.md");
    writeFileAtomic(path, "hola\n", "a/b/doc.md");
    assert.equal(readFileSync(path, "utf8"), "hola\n");
    assert.deepEqual(readdirSync(join(dir, "a", "b")), ["doc.md"]);
  }));

test("fs_escritura_atomica_sustituye_el_contenido", () =>
  withDir((dir) => {
    const path = join(dir, "doc.md");
    writeFileSync(path, "viejo\n");
    writeFileAtomic(path, "nuevo\n", "doc.md");
    assert.equal(readFileSync(path, "utf8"), "nuevo\n");
  }));

test("fs_escritura_fallida_conserva_el_original_y_nombra_la_ruta", () =>
  withDir((dir) => {
    const path = join(dir, "doc.md");
    writeFileSync(path, "viejo\n");
    mkdirSync(join(dir, ".doc.tmp"));
    assert.throws(
      () => writeFileAtomic(path, "nuevo\n", "docs/doc.md"),
      (error) => error instanceof PreconditionError && /no se puede escribir docs\/doc\.md/.test(error.message),
    );
    assert.equal(readFileSync(path, "utf8"), "viejo\n");
  }));

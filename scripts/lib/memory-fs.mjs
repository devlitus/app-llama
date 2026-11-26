import { mkdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, extname, join } from "node:path";
import { PreconditionError } from "./memory-errors.mjs";

// La limpieza no debe tapar el error original de escritura.
function discardTemporary(path) {
  try {
    rmSync(path, { force: true });
  } catch {
    // Si no se puede borrar, sobrevive un temporal oculto que la siguiente escritura sobrescribe.
  }
}

// Temporal oculto en el mismo directorio y rename: nunca queda un fichero a medio escribir.
export function writeFileAtomic(path, content, rel) {
  const temporary = join(dirname(path), `.${basename(path, extname(path))}.tmp`);
  try {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(temporary, content, "utf8");
    renameSync(temporary, path);
  } catch (error) {
    discardTemporary(temporary);
    throw new PreconditionError(`no se puede escribir ${rel}: ${error.message}`);
  }
}

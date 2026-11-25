import { join } from "node:path";
import { writeFileAtomic } from "./memory-fs.mjs";
import { renderLong, selectLongEntries } from "./memory-long-format.mjs";
import { listMediumPlans, readMedium } from "./memory-medium.mjs";

export const LONG_REL = "docs/memory/long";

function writeLong(root, kind, entries) {
  const rel = `${LONG_REL}/${kind}.md`;
  writeFileAtomic(join(root, "docs", "memory", "long", `${kind}.md`), renderLong(kind, entries), rel);
  return rel;
}

// Se lee todo el medio plazo antes de escribir: un fichero ilegible no deja el largo plazo a medias.
// Repetir `promote` tras un fallo de escritura recupera, porque regenera ambos ficheros enteros.
export function promote(root) {
  const docs = listMediumPlans(root).map((plan) => readMedium(root, plan).doc);
  const { decisions, errors, dropped } = selectLongEntries(docs);
  const written = [writeLong(root, "decisiones", decisions), writeLong(root, "errores", errors)];
  return { written, dropped };
}

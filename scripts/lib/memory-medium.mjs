import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { PreconditionError } from "./memory-errors.mjs";
import { writeFileAtomic } from "./memory-fs.mjs";
import { comparePlans } from "./memory-order.mjs";
import { parsePlan, renderPlan } from "./memory-plan.mjs";

export const MEDIUM_REL = "docs/memory/medium";
const PLAN_FILE_RE = /^(\d{3}-[a-z0-9]+(?:-[a-z0-9]+)*)\.md$/;

const mediumDir = (root) => join(root, "docs", "memory", "medium");
const mediumPath = (root, plan) => join(mediumDir(root), `${plan}.md`);

export function mediumExists(root, plan) {
  return existsSync(mediumPath(root, plan));
}

function parseMedium(text, plan, rel) {
  try {
    const doc = parsePlan(text);
    if (doc.plan !== plan) throw new Error(`el plan del front matter es '${doc.plan}', no '${plan}'`);
    return doc;
  } catch (error) {
    throw new PreconditionError(`medio plazo ilegible ${rel}: ${error.message}`, { cause: error });
  }
}

// Sin fichero, el medio plazo es un plan abierto vacío: así `flush` lo crea al primer volcado.
export function readMedium(root, plan) {
  const rel = `${MEDIUM_REL}/${plan}.md`;
  if (!mediumExists(root, plan)) return { rel, doc: { plan, estado: "abierto", tasks: [] } };
  let text;
  try {
    text = readFileSync(mediumPath(root, plan), "utf8");
  } catch (error) {
    throw new PreconditionError(`medio plazo ilegible ${rel}: ${error.message}`, { cause: error });
  }
  return { rel, doc: parseMedium(text, plan, rel) };
}

export function writeMedium(root, plan, doc) {
  writeFileAtomic(mediumPath(root, plan), renderPlan(doc), `${MEDIUM_REL}/${plan}.md`);
}

function mediumFileNames(root) {
  if (!existsSync(mediumDir(root))) return [];
  return readdirSync(mediumDir(root)).filter((name) => name.endsWith(".md") && !name.startsWith("."));
}

// Separa los planes válidos de los ficheros con nombre no válido, para que `check` informe de ambos.
export function scanMedium(root) {
  const names = mediumFileNames(root);
  const plans = names.flatMap((name) => PLAN_FILE_RE.exec(name)?.[1] ?? []).sort(comparePlans);
  return { plans, invalid: names.filter((name) => !PLAN_FILE_RE.test(name)).sort() };
}

export function listMediumPlans(root) {
  const { plans, invalid } = scanMedium(root);
  if (invalid.length > 0) throw new PreconditionError(`nombre de medio plazo no válido ${MEDIUM_REL}/${invalid[0]}`);
  return plans;
}

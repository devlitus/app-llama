import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { PreconditionError } from "./memory-errors.mjs";
import { writeFileAtomic } from "./memory-fs.mjs";
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
    throw new PreconditionError(`medio plazo ilegible ${rel}: ${error.message}`);
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
    throw new PreconditionError(`medio plazo ilegible ${rel}: ${error.message}`);
  }
  return { rel, doc: parseMedium(text, plan, rel) };
}

export function writeMedium(root, plan, doc) {
  writeFileAtomic(mediumPath(root, plan), renderPlan(doc), `${MEDIUM_REL}/${plan}.md`);
}

function planOfFileName(name) {
  const plan = PLAN_FILE_RE.exec(name)?.[1];
  if (!plan) throw new PreconditionError(`nombre de medio plazo no válido ${MEDIUM_REL}/${name}`);
  return plan;
}

const planNumber = (plan) => Number(plan.slice(0, 3));

export function listMediumPlans(root) {
  if (!existsSync(mediumDir(root))) return [];
  return readdirSync(mediumDir(root))
    .filter((name) => name.endsWith(".md") && !name.startsWith("."))
    .map(planOfFileName)
    .sort((a, b) => planNumber(a) - planNumber(b) || (a < b ? -1 : 1));
}

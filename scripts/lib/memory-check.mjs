import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { PreconditionError } from "./memory-errors.mjs";
import { planIssues, tagLines } from "./memory-check-format.mjs";
import { renderLong, selectLongEntries } from "./memory-long-format.mjs";
import { LONG_REL } from "./memory-long.mjs";
import { MEDIUM_REL, listMediumPlans, readMedium, scanMedium } from "./memory-medium.mjs";
import { comparePlans } from "./memory-order.mjs";

const GITIGNORE_LINE = "docs/memory/short/";
const PLAN_FILE_RE = /^\d{3}-.*\.md$/;
const DONE_RE = /^- \*\*Estado:\*\* hecho\s*$/m;
const LONG_KINDS = [
  { kind: "decisiones", field: "decisions" },
  { kind: "errores", field: "errors" },
];

const normalize = (text) => text.replace(/\r\n/g, "\n");
const readText = (path) => normalize(readFileSync(path, "utf8"));
const reasonOf = (error) => (error.cause ?? error).message;

function gitignoreIssues(root) {
  const path = join(root, ".gitignore");
  const lines = existsSync(path) ? readText(path).split("\n") : [];
  return lines.includes(GITIGNORE_LINE) ? [] : [`.gitignore: falta la línea ${GITIGNORE_LINE}`];
}

function loadMediumPlan(root, plan) {
  try {
    return { loaded: readMedium(root, plan) };
  } catch (error) {
    if (!(error instanceof PreconditionError)) throw error;
    return { issue: `${MEDIUM_REL}/${plan}.md: ${reasonOf(error)}` };
  }
}

// `readable` es falso si algún medio plazo no se puede leer: sin él no se puede regenerar el largo plazo.
function loadMedium(root) {
  const { plans, invalid } = scanMedium(root);
  const results = plans.map((plan) => loadMediumPlan(root, plan));
  const docs = results.filter(({ loaded }) => loaded).map(({ loaded }) => loaded);
  const unreadable = results.filter(({ issue }) => issue).map(({ issue }) => issue);
  const invalidNames = invalid.map((name) => `${MEDIUM_REL}/${name}: nombre de medio plazo no válido`);
  return { docs, issues: [...invalidNames, ...unreadable], readable: unreadable.length === 0 };
}

const mediumIssues = ({ rel, doc }) => planIssues(doc).map((issue) => `${rel}: ${issue}`);

// Sin fichero, el largo plazo equivale a estar vacío: un plan abierto aún no lo ha generado.
function longIssue(root, kind, expected) {
  const rel = `${LONG_REL}/${kind}.md`;
  const path = join(root, ...rel.split("/"));
  const actual = existsSync(path) ? readText(path) : renderLong(kind, []);
  return actual === expected ? [] : [`${rel}: desactualizado; ejecuta node scripts/memory.mjs promote`];
}

function longIssues(root, docs) {
  const selected = selectLongEntries(docs);
  return LONG_KINDS.flatMap(({ kind, field }) => longIssue(root, kind, renderLong(kind, selected[field])));
}

function donePlanIssues(root, docs) {
  const plansDir = join(root, "docs", "plans");
  if (!existsSync(plansDir)) return [];
  const openPlans = new Set(docs.filter(({ doc }) => doc.estado === "abierto").map(({ doc }) => doc.plan));
  return readdirSync(plansDir)
    .filter((name) => PLAN_FILE_RE.test(name) && openPlans.has(name.slice(0, -3)))
    .sort(comparePlans)
    .filter((name) => DONE_RE.test(readText(join(plansDir, name))))
    .map((name) => `docs/plans/${name}: plan hecho con el medio plazo abierto; ejecuta node scripts/memory.mjs close`);
}

// No lee `short/`: las sesiones son efímeras y no existen en la CI.
export function check(root) {
  const { docs, issues, readable } = loadMedium(root);
  const all = [
    ...gitignoreIssues(root),
    ...issues,
    ...docs.flatMap(mediumIssues),
    ...(readable ? longIssues(root, docs.map(({ doc }) => doc)) : []),
    ...donePlanIssues(root, docs),
  ];
  return { issues: all, plans: docs.length, closed: docs.filter(({ doc }) => doc.estado === "cerrado").length };
}

export function tags(root) {
  return tagLines(listMediumPlans(root).map((plan) => readMedium(root, plan).doc));
}

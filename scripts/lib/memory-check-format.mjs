import { compareBySlug } from "./memory-order.mjs";
import { lastCycle } from "./memory-plan.mjs";

const TAG_KINDS = [
  { kind: "decision", field: "decisions" },
  { kind: "error", field: "errors" },
];

function gapIssues({ task, cycles }) {
  const numbers = cycles.map(({ cycle }) => cycle);
  const consecutive = numbers.every((number, index) => number === index + 1);
  return consecutive ? [] : [`${task}: ciclos no consecutivos (${numbers.join(", ")})`];
}

function approvalIssues(task) {
  return lastCycle(task)?.veredicto === "APPROVED" ? [] : [`${task.task}: el último ciclo no es APPROVED en un plan cerrado`];
}

// Incidencias de un medio plazo ya leído (y por tanto canónico), sin la ruta.
export function planIssues(doc) {
  const approval = doc.estado === "cerrado" ? approvalIssues : () => [];
  return doc.tasks.flatMap((task) => [...gapIssues(task), ...approval(task)]);
}

function slugsOf(doc, field) {
  const entries = doc.estado === "cerrado" ? doc.patterns[field] : doc.tasks.flatMap((task) => task[field]);
  return new Set(entries.map(({ slug }) => slug));
}

function countPlansBySlug(docs, field) {
  const counts = new Map();
  for (const doc of docs) {
    for (const slug of slugsOf(doc, field)) counts.set(slug, (counts.get(slug) ?? 0) + 1);
  }
  return [...counts].map(([slug, plans]) => ({ slug, plans }));
}

const byPlansThenSlug = (a, b) => b.plans - a.plans || compareBySlug(a, b);

// Una línea `<tipo> <slug> <planes>` por slug: tipo, nº de planes descendente y slug.
export function tagLines(docs) {
  return TAG_KINDS.flatMap(({ kind, field }) =>
    countPlansBySlug(docs, field)
      .sort(byPlansThenSlug)
      .map(({ slug, plans }) => `${kind} ${slug} ${plans}`),
  );
}

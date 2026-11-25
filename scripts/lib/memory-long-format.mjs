const SCHEMA = "memoria-larga/1";
const MIN_PLANS = 3;
const MAX_ENTRIES = 40;

const FILES = {
  decisiones: { title: "Decisiones recurrentes", empty: "- (ninguna)" },
  errores: { title: "Errores recurrentes", empty: "- (ninguno)" },
};

function renderEntry({ slug, text, plans }) {
  return `- **${slug}** — ${text} (planes: ${plans.join(", ")})`;
}

// 40 entradas más las 9 líneas de cabecera dan 49 líneas por fichero, 98 entre los dos.
export function renderLong(kind, entries) {
  const { title, empty } = FILES[kind];
  const lines = entries.length > 0 ? entries.map(renderEntry) : [empty];
  const header = [
    "---",
    `schema: ${SCHEMA}`,
    `tipo: ${kind}`,
    "---",
    "",
    `# ${title}`,
    "",
    "<!-- Generado por `node scripts/memory.mjs promote`. No editar a mano. -->",
    "",
  ];
  return `${[...header, ...lines].join("\n")}\n`;
}

const planNumber = (plan) => Number(plan.slice(0, 3));
const comparePlans = (a, b) => planNumber(a) - planNumber(b) || (a < b ? -1 : a > b ? 1 : 0);

function groupBySlug(closedDocs, field) {
  const groups = new Map();
  for (const doc of [...closedDocs].sort((a, b) => comparePlans(a.plan, b.plan))) {
    for (const { slug, text } of doc.patterns[field]) {
      const plans = [...(groups.get(slug)?.plans ?? []), doc.plan];
      groups.set(slug, { slug, text, plans });
    }
  }
  return [...groups.values()];
}

const latestPlan = ({ plans }) => plans.at(-1);

function compareEntries(a, b) {
  return (
    b.plans.length - a.plans.length ||
    comparePlans(latestPlan(b), latestPlan(a)) ||
    (a.slug < b.slug ? -1 : a.slug > b.slug ? 1 : 0)
  );
}

function selectKind(closedDocs, field, kind) {
  const ranked = groupBySlug(closedDocs, field)
    .filter(({ plans }) => plans.length >= MIN_PLANS)
    .sort(compareEntries);
  return {
    kept: ranked.slice(0, MAX_ENTRIES),
    dropped: ranked.slice(MAX_ENTRIES).map(({ slug }) => ({ kind, slug })),
  };
}

// Solo cuentan los planes cerrados; el texto de cada slug es el del plan más reciente.
export function selectLongEntries(docs) {
  const closedDocs = docs.filter(({ estado }) => estado === "cerrado");
  const decisions = selectKind(closedDocs, "decisions", "decision");
  const errors = selectKind(closedDocs, "errors", "error");
  return { decisions: decisions.kept, errors: errors.kept, dropped: [...decisions.dropped, ...errors.dropped] };
}

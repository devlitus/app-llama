import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { PreconditionError } from "./memory-errors.mjs";
import { mergeCycle, parsePlan, renderPlan } from "./memory-plan.mjs";
import { sessionsOfTask } from "./memory-store.mjs";

const MEDIUM_REL = "docs/memory/medium";

function mediumPath(root, plan) {
  return join(root, "docs", "memory", "medium", `${plan}.md`);
}

function readMedium(root, plan) {
  const rel = `${MEDIUM_REL}/${plan}.md`;
  const path = mediumPath(root, plan);
  if (!existsSync(path)) return { rel, doc: { plan, estado: "abierto", tasks: [] } };
  try {
    return { rel, doc: parsePlan(readFileSync(path, "utf8")) };
  } catch (error) {
    throw new PreconditionError(`medio plazo ilegible ${rel}: ${error.message}`);
  }
}

// La limpieza no debe tapar el error original de escritura.
function discardTemporary(path) {
  try {
    rmSync(path, { force: true });
  } catch {
    // Si no se puede borrar, sobrevive un temporal oculto que el siguiente flush sobrescribe.
  }
}

// Temporal oculto en el mismo directorio y rename: nunca queda un medio plazo a medio escribir.
function writeMedium(root, plan, doc) {
  const path = mediumPath(root, plan);
  const temporary = join(dirname(path), `.${plan}.tmp`);
  try {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(temporary, renderPlan(doc), "utf8");
    renameSync(temporary, path);
  } catch (error) {
    discardTemporary(temporary);
    throw new PreconditionError(`no se puede escribir ${MEDIUM_REL}/${plan}.md: ${error.message}`);
  }
}

function pickReviewer(sessions, task, agentId) {
  const reviewers = sessions.filter(({ meta }) => meta.agente === "reviewer");
  if (reviewers.length !== 1) {
    throw new PreconditionError(`se esperaba una sesión del reviewer de ${task} y hay ${reviewers.length}`);
  }
  if (reviewers[0].meta.agentId !== agentId) {
    throw new PreconditionError(`la sesión del reviewer de ${task} es de '${reviewers[0].meta.agentId}', no de '${agentId}'`);
  }
  return reviewers[0];
}

function deleteSession(root, { rel }) {
  try {
    rmSync(join(root, ...rel.split("/")));
  } catch (error) {
    throw new PreconditionError(`no se puede borrar ${rel}: ${error.message}`);
  }
}

function deleteSessions(root, devSessions, reviewerSession) {
  const ordered = [...devSessions, reviewerSession];
  ordered.forEach((session) => deleteSession(root, session));
  return ordered.length;
}

function recordCycle(root, plan, doc, task, devSessions, reviewerSession) {
  const known = doc.tasks
    .find((entry) => entry.task === task)
    ?.cycles.find(({ reviewer }) => reviewer === reviewerSession.meta.agentId);
  if (known) return known.cycle;
  const merged = mergeCycle(doc, task, devSessions, reviewerSession);
  writeMedium(root, plan, merged);
  return merged.tasks.find((entry) => entry.task === task).cycles.length;
}

// Volcar y borrar son dos pasos: si el borrado falla, repetir el flush recupera sin duplicar el ciclo.
export function flush(root, plan, task, agentId) {
  const { rel, doc } = readMedium(root, plan);
  if (doc.estado === "cerrado") throw new PreconditionError(`el plan ${plan} está cerrado: ${rel}`);
  const sessions = sessionsOfTask(root, plan, task);
  const reviewerSession = pickReviewer(sessions, task, agentId);
  const devSessions = sessions.filter(({ meta }) => meta.agente === "developer");
  const cycle = recordCycle(root, plan, doc, task, devSessions, reviewerSession);
  const deleted = deleteSessions(root, devSessions, reviewerSession);
  return { rel, cycle, deleted };
}

import { planOfBranch } from "./memory-branch.mjs";
import { PreconditionError } from "./memory-errors.mjs";
import { readMedium } from "./memory-medium.mjs";
import { flushOrder, sessionOrderLines } from "./memory-orders.mjs";
import { findSessionByAgentId, sessionsOfTask } from "./memory-store.mjs";

const PASS = { block: false };

const blockWith = (reason) => ({ block: true, reason });

function missingSessionDecision(agentType, agentId) {
  const header = "falta tu entrada de sesión en docs/memory/short (R11). Ejecútala con la herramienta Bash antes de terminar:";
  return blockWith([header, ...sessionOrderLines(agentType, agentId)].join("\n"));
}

function pendingFlushDecision(task, agentId) {
  const header = `quedan sesiones de ${task} sin volcar en docs/memory/medium (R11). Ejecuta con la herramienta Bash antes de terminar:`;
  return blockWith([header, flushOrder(task, agentId)].join("\n"));
}

function developerDecision(root, agentId) {
  return findSessionByAgentId(root, agentId) ? PASS : missingSessionDecision("developer", agentId);
}

// Tarea cuyo ciclo ya registró este reviewer en el medio plazo, o undefined si no ha volcado nada.
function flushedTask(root, plan, agentId) {
  const { doc } = readMedium(root, plan);
  return doc.tasks.find(({ cycles }) => cycles.some(({ reviewer }) => reviewer === agentId))?.task;
}

function afterFlushDecision(root, plan, agentId) {
  const task = flushedTask(root, plan, agentId);
  if (task === undefined) return missingSessionDecision("reviewer", agentId);
  return sessionsOfTask(root, plan, task).length > 0 ? pendingFlushDecision(task, agentId) : PASS;
}

function reviewerDecision(root, plan, agentId) {
  const pending = findSessionByAgentId(root, agentId);
  if (!pending) return afterFlushDecision(root, plan, agentId);
  if (!pending.meta) return blockWith(`tu sesión ${pending.rel} es ilegible (R11): no se puede volcar`);
  return pendingFlushDecision(pending.meta.tarea, agentId);
}

function decide(input, { root, plan }) {
  const { agent_type: agentType, agent_id: agentId } = input;
  if (agentType === "developer") return developerDecision(root, agentId);
  if (agentType === "reviewer") return reviewerDecision(root, plan, agentId);
  return PASS;
}

// Un fallo de lectura se convierte en bloqueo explícito: nunca se deja terminar por no poder comprobar.
export function stopDecisionFor(input, { root, branch }) {
  const plan = planOfBranch(branch);
  if (!plan) return PASS;
  try {
    return decide(input, { root, plan });
  } catch (error) {
    if (!(error instanceof PreconditionError)) throw error;
    return blockWith(`no se puede comprobar la memoria (R11): ${error.message}`);
  }
}

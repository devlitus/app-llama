import { PreconditionError } from "./memory-errors.mjs";
import { promote } from "./memory-long.mjs";
import { mediumExists, readMedium, writeMedium } from "./memory-medium.mjs";
import { compactPlan } from "./memory-plan.mjs";
import { listSessions } from "./memory-store.mjs";

const taskNumber = (task) => Number(task.slice(1));

function lastCycle({ cycles }) {
  return cycles.reduce((latest, current) => (latest === undefined || current.cycle > latest.cycle ? current : latest), undefined);
}

function requireAllApproved(doc) {
  const pending = doc.tasks
    .filter((task) => lastCycle(task)?.veredicto !== "APPROVED")
    .sort((a, b) => taskNumber(a.task) - taskNumber(b.task))
    .map(({ task }) => task);
  if (pending.length > 0) {
    throw new PreconditionError(`el último ciclo de ${pending.join(", ")} no es APPROVED: no se puede cerrar el plan ${doc.plan}`);
  }
}

function requireNoPendingSessions(root, plan) {
  const pending = listSessions(root).filter(({ meta }) => meta.plan === plan);
  if (pending.length > 0) {
    const names = pending.map(({ rel }) => rel).join(", ");
    throw new PreconditionError(`quedan sesiones pendientes de ${plan} en docs/memory/short: ${names}`);
  }
}

// Compactar y promover son dos pasos: si `promote` falla, repetir `close` recupera, porque un plan
// ya cerrado no se reescribe y solo regenera el largo plazo.
export function close(root, plan) {
  if (!mediumExists(root, plan)) throw new PreconditionError(`no existe el medio plazo docs/memory/medium/${plan}.md`);
  const { rel, doc } = readMedium(root, plan);
  const compacted = doc.estado === "abierto";
  if (compacted) {
    requireAllApproved(doc);
    requireNoPendingSessions(root, plan);
    writeMedium(root, plan, compactPlan(doc));
  }
  return { rel, compacted, long: promote(root) };
}

import { PreconditionError } from "./memory-errors.mjs";
import { promote } from "./memory-long.mjs";
import { MEDIUM_REL, mediumExists, readMedium, writeMedium } from "./memory-medium.mjs";
import { compareTasks } from "./memory-order.mjs";
import { compactPlan, lastCycle } from "./memory-plan.mjs";
import { listSessions } from "./memory-store.mjs";

function requireAllApproved(doc) {
  const pending = doc.tasks
    .filter((task) => lastCycle(task)?.veredicto !== "APPROVED")
    .sort((a, b) => compareTasks(a.task, b.task))
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
  if (!mediumExists(root, plan)) throw new PreconditionError(`no existe el medio plazo ${MEDIUM_REL}/${plan}.md`);
  const { rel, doc } = readMedium(root, plan);
  const compacted = doc.estado === "abierto";
  if (compacted) {
    requireAllApproved(doc);
    requireNoPendingSessions(root, plan);
    writeMedium(root, plan, compactPlan(doc));
  }
  return { rel, compacted, long: promote(root) };
}

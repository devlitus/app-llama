const VERDICT_ARG = " --veredicto <APPROVED|CHANGES_REQUESTED>";

// Líneas de la orden `session` (desde `node …` hasta `EOF`) tal como las ejecuta cada tipo de agente.
export function sessionOrderLines(agentType, agentId) {
  const verdict = agentType === "reviewer" ? VERDICT_ARG : "";
  return [
    `node scripts/memory.mjs session --agent ${agentType} --agent-id ${agentId} --task <Tn>${verdict} <<'EOF'`,
    "resumen: <una línea>",
    "decision: [<slug>] <una línea>",
    "error: [<slug>] <una línea; enlaza #N si hay issue>",
    "EOF",
  ];
}

export const flushOrder = (task, agentId) => `node scripts/memory.mjs flush --task ${task} --agent-id ${agentId}`;

export function reviewerFollowUpLines(agentId) {
  return [
    "Después vuelca la tarea, tanto con APPROVED como con CHANGES_REQUESTED:",
    flushOrder("<Tn>", agentId),
    'Si el orchestrator indica "última tarea: sí" y el veredicto es APPROVED, después:',
    "node scripts/memory.mjs close",
  ];
}

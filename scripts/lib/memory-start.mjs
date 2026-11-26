import { SESSION_AGENTS } from "./memory-agents.mjs";
import { reviewerFollowUpLines, sessionOrderLines } from "./memory-orders.mjs";

const MAX_TAG_LINES = 30;
const NO_TAGS = "(ninguno)";
const EMPTY_LONG = "[R11] La memoria a largo plazo está vacía.";

function tagsBlock(tagLines) {
  return tagLines.length > 0 ? tagLines.slice(0, MAX_TAG_LINES).join("\n") : NO_TAGS;
}

function sessionAgentContext(agentType, agentId, tagLines) {
  const followUp = agentType === "reviewer" ? reviewerFollowUpLines(agentId) : [];
  return [
    `[R11] agent_id de esta sesión: ${agentId}`,
    "Antes de terminar registra tu sesión con la herramienta Bash, con esta forma exacta:",
    ...sessionOrderLines(agentType, agentId),
    ...followUp,
    "Slugs existentes (reutilízalos si encajan):",
    tagsBlock(tagLines),
  ].join("\n");
}

function plannerContext(longTexts) {
  if (longTexts.length === 0) return EMPTY_LONG;
  return ["[R11] Memoria a largo plazo (léela antes de planificar):", "", longTexts.join("\n\n")].join("\n");
}

// Las lecturas se piden como funciones: solo se toca el disco para el tipo de agente que las necesita.
export function startContextFor(input, { readTagLines, readLongTexts }) {
  const { agent_type: agentType, agent_id: agentId } = input;
  if (SESSION_AGENTS.includes(agentType)) return sessionAgentContext(agentType, agentId, readTagLines());
  if (agentType === "planner") return plannerContext(readLongTexts());
  return null;
}

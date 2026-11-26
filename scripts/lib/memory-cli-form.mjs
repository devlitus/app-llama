import { SESSION_AGENTS } from "./memory-agents.mjs";

const INVOCATION =
  /^node scripts\/memory\.mjs (session|flush|close|promote|check|tags|help)((?: --[a-z][a-z-]* [A-Za-z0-9._-]+)*)( <<'EOF')?$/;
const HEREDOC_END = "EOF";
const READ_ONLY = ["check", "tags", "help"];
const REVIEWER_ONLY = ["close", "promote"];
const ALLOW = { allow: true };

const deny = (reason) => ({ allow: false, reason });

function parseOptions(text) {
  const words = text.split(" ").filter(Boolean);
  const options = new Map();
  for (let i = 0; i < words.length; i += 2) options.set(words[i].slice(2), words[i + 1]);
  return options;
}

function hasValidBody(lines, heredoc, subcommand) {
  if (!heredoc) return lines.length === 1;
  const terminators = lines.slice(1).filter((line) => line === HEREDOC_END);
  return subcommand === "session" && lines.at(-1) === HEREDOC_END && terminators.length === 1;
}

// Forma exacta de una invocación de memory.mjs; null si el comando no la cumple.
export function parseInvocation(command) {
  const lines = command.split("\n");
  const match = INVOCATION.exec(lines[0]);
  if (!match || !hasValidBody(lines, Boolean(match[3]), match[1])) return null;
  return { line: lines[0], subcommand: match[1], options: parseOptions(match[2]) };
}

function checkSession({ agent_type: agentType, agent_id: agentId }, options) {
  const identified = SESSION_AGENTS.includes(agentType);
  const own = options.get("agent") === agentType && options.get("agent-id") === agentId;
  return identified && own ? ALLOW : deny("R11: session solo la ejecuta developer o reviewer con su propio --agent y --agent-id");
}

function checkFlush({ agent_type: agentType, agent_id: agentId }, options) {
  const own = agentType === "reviewer" && options.get("agent-id") === agentId;
  return own ? ALLOW : deny("R11: flush solo lo ejecuta el reviewer con su propio --agent-id");
}

function checkReviewerOnly({ agent_type: agentType }, subcommand) {
  return agentType === "reviewer" ? ALLOW : deny(`R11: ${subcommand} solo lo ejecuta el reviewer`);
}

// Permisos por subcomando y agente (D6) sobre una invocación que ya cumple la forma exacta.
export function checkInvocation(input, { subcommand, options }) {
  if (input.tool_name === "PowerShell" && !READ_ONLY.includes(subcommand)) {
    return deny(`R11: ${subcommand} se ejecuta con la herramienta Bash: usa la herramienta Bash`);
  }
  if (subcommand === "session") return checkSession(input, options);
  if (subcommand === "flush") return checkFlush(input, options);
  if (REVIEWER_ONLY.includes(subcommand)) return checkReviewerOnly(input, subcommand);
  return ALLOW;
}

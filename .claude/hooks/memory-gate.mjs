// SubagentStart / SubagentStop: memoria del equipo. R11.
// Start: inyecta el agent_id y las órdenes de memoria (developer, reviewer) o el largo plazo (planner).
// Stop: no deja terminar a developer sin su sesión ni a reviewer sin su sesión volcada.

import { readHookInput, block, git, PROJECT_DIR } from "./lib.mjs";
import { subagentStartContext, subagentStopDecision } from "../../scripts/lib/memory-policy.mjs";

const input = readHookInput();

if (input.hook_event_name === "SubagentStart") {
  const context = subagentStartContext(input, { root: PROJECT_DIR });
  if (context) {
    process.stdout.write(
      JSON.stringify({ hookSpecificOutput: { hookEventName: "SubagentStart", additionalContext: context } }),
    );
  }
  process.exit(0);
}

if (input.hook_event_name === "SubagentStop") {
  const decision = subagentStopDecision(input, { root: PROJECT_DIR, branch: git(["branch", "--show-current"]) });
  if (decision.block) block(decision.reason);
}

process.exit(0);

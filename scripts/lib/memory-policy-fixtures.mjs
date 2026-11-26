import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const developer = { agent_type: "developer", agent_id: "a1" };
export const reviewer = { agent_type: "reviewer", agent_id: "r1" };

export function withRoot(body) {
  const root = mkdtempSync(join(tmpdir(), "mem-"));
  try {
    return body(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

export const bash = (command, identity = developer, tool_name = "Bash") => ({ ...identity, tool_name, tool_input: { command } });

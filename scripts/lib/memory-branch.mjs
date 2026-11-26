const BRANCH_RE = /^(?:feat|fix|chore)\/(\d{3}-[a-z0-9]+(?:-[a-z0-9]+)*)$/;

// El plan de una rama es su `NNN-slug` (R10); null si la rama no corresponde a ningún plan.
export function planOfBranch(branch) {
  return BRANCH_RE.exec(branch ?? "")?.[1] ?? null;
}

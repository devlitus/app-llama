export const SECRET_PATTERNS = [
  { kind: "hugging-face", re: /hf_[A-Za-z0-9]{30,}/ },
  { kind: "github", re: /gh[pousr]_[A-Za-z0-9]{36,}/ },
  { kind: "github", re: /github_pat_[A-Za-z0-9_]{22,}/ },
  { kind: "anthropic", re: /sk-ant-[A-Za-z0-9_-]{20,}/ },
  { kind: "aws", re: /AKIA[0-9A-Z]{16}/ },
  { kind: "private-key", re: /-{5}BEGIN [A-Z ]*PRIVATE KEY-{5}/ },
];

function findSecretsInLine(text, line) {
  return SECRET_PATTERNS.filter(({ re }) => re.test(text)).map(({ kind }) => ({ line, kind }));
}

export function findSecrets(text) {
  return text
    .split(/\r?\n/)
    .flatMap((lineText, index) => findSecretsInLine(lineText, index + 1));
}

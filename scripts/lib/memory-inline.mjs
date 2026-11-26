// Detecta código en línea (node -e, python -c, perl -pi, iex…): no se puede auditar, así que se trata como escritura.
const P = "[a-z0-9_.-]";
const INTERPRETER = new RegExp(
  `(?<!${P})(node|nodejs|bun|deno|tsx|ts-node|python[0-9.]*|py|perl|ruby|php|sh|bash|zsh|dash|pwsh|powershell|cmd)(\\.exe)?(?!${P})`,
);
const INLINE_FLAG = /^(-[a-z]*[ecpi][a-z]*|--eval|--print|-command|-encodedcommand|\/c|eval)$/;
const INVOKE_EXPRESSION = new RegExp(`(?<!${P})(iex|invoke-expression)(?!${P})`);

function segmentRunsInlineCode(segment) {
  const interpreter = INTERPRETER.exec(segment);
  if (!interpreter) return false;
  const words = segment.slice(interpreter.index + interpreter[0].length).split(/\s+/).filter(Boolean);
  return words.some((word) => INLINE_FLAG.test(word));
}

export function runsInlineCode(command) {
  const lower = command.toLowerCase();
  return INVOKE_EXPRESSION.test(lower) || lower.split(/[;&|\n]/).some(segmentRunsInlineCode);
}

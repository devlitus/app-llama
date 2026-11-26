// Detecta si un comando de shell menciona la memoria (docs/memory) o la CLI que la escribe, aunque esté ofuscado.
const P = "[a-z0-9_.-]";

const DOCS_WORD = new RegExp(`(?<!${P})docs(?=$|[/\\s;&|])`);
const MEMORY_WORD = new RegExp(`(?<!${P})memory(?!${P})`);
const MENTIONS = [
  new RegExp(`docs/memory(?!${P})`),
  { all: [DOCS_WORD, MEMORY_WORD] },
  /docs\/[^\s/;&|]*[*?[{]/,
  new RegExp(`[^\\s/;&|]*[*?[{][^\\s/;&|]*/memory(?!${P})`),
  new RegExp(`docs/m(e(m(o(r)?)?)?)?(?!${P})`),
  { all: [/\$[a-z_{(]|%[a-z_]+%/, { any: [DOCS_WORD, MEMORY_WORD] }] },
];
const CLI_MENTION = new RegExp(`(?<!${P})memory[.-]`);

function matches(pattern, text) {
  if (pattern instanceof RegExp) return pattern.test(text);
  if (pattern.all) return pattern.all.every((part) => matches(part, text));
  return pattern.any.some((part) => matches(part, text));
}

const stripDotSegments = (text) => text.replace(/(^|[/\s=])\.\//g, "$1");
const resolveParents = (text) => text.replace(/([^/\s]+)\/\.\.\//g, (whole, segment) => (segment === ".." ? whole : ""));

function normalizePath(text) {
  let current = text;
  for (;;) {
    const next = resolveParents(stripDotSegments(current.replace(/\/{2,}/g, "/")));
    if (next === current) return current;
    current = next;
  }
}

// `\` es separador en Windows (variante A) y escape en bash (variante B): basta con que una variante mencione la memoria.
function variantsOf(command) {
  const plain = command.toLowerCase().replace(/['"`]/g, "");
  return [plain.replace(/\\/g, "/"), plain.replace(/\\/g, "")].map(normalizePath);
}

export function touchesMemory(command) {
  const variants = variantsOf(command);
  return variants.some((variant) => CLI_MENTION.test(variant) || MENTIONS.some((mention) => matches(mention, variant)));
}

export function mentionsCli(command) {
  return variantsOf(command).some((variant) => CLI_MENTION.test(variant));
}

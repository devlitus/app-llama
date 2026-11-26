// Analizador léxico mínimo de Bash. Ante cualquier construcción que no sabe
// interpretar con certeza devuelve null, y cada llamador aplica su regla conservadora.

// Los comodines sin comillas los expande Bash: el resultado real es desconocido.
const UNSUPPORTED = new Set(["$", "`", "(", ")", "}", "*", "?", "["]);
const BLANKS = new Set([" ", "\t"]);
const DIGITS = /^\d+$/;

// Un fallo del lector se señala con FAIL, que no es un índice válido.
const FAIL = -1;

class Word {
  value = "";
  started = false;
  bare = true; // sin comillas ni escapes: solo así un número es prefijo de descriptor

  append(text, { bare = false } = {}) {
    this.value += text;
    this.started = true;
    this.bare &&= bare;
  }

  isFileDescriptor() {
    return this.started && this.bare && DIGITS.test(this.value);
  }
}

function readSingleQuoted(src, i, word) {
  const end = src.indexOf("'", i + 1);
  if (end === -1) return FAIL;
  word.append(src.slice(i + 1, end));
  return end + 1;
}

function readDoubleQuoted(src, i, word) {
  const end = src.indexOf('"', i + 1);
  if (end === -1) return FAIL;
  const content = src.slice(i + 1, end);
  if (/[$`\\]/.test(content)) return FAIL;
  word.append(content);
  return end + 1;
}

function readEscape(src, i, word) {
  const next = src[i + 1];
  if (next === undefined || next === "'" || next === '"') return FAIL;
  if (next !== "\n") word.append(next);
  return i + 2;
}

function readBrace(src, i, word) {
  if (src[i + 1] !== "}") return FAIL;
  word.append("{}", { bare: true });
  return i + 2;
}

function redirectOperator(src, i) {
  const rest = src.slice(i);
  if (src[i] === "&") return /^&>>?/.exec(rest)?.[0] ?? null;
  if (src[i] === ">") return /^>(?:>|\||&)?/.exec(rest)[0];
  if (rest.startsWith("<<<")) return "<<<";
  if (rest.startsWith("<<")) return null;
  return /^<(?:&|>)?/.exec(rest)[0];
}

function separatorLength(src, i) {
  const pair = src.slice(i, i + 2);
  if (pair === "&&" || pair === "||" || pair === "|&") return 2;
  return 1;
}

function tokenize(src) {
  const tokens = [];
  let word = new Word();

  const flush = () => {
    if (word.started) tokens.push({ type: "word", value: word.value });
    word = new Word();
  };

  const pushRedirect = (op) => {
    const prefix = !op.startsWith("&") && word.isFileDescriptor() ? word.value : "";
    if (prefix) word = new Word();
    flush();
    tokens.push({ type: "redirect", op: prefix + op });
  };

  const readers = { "'": readSingleQuoted, '"': readDoubleQuoted, "\\": readEscape, "{": readBrace };

  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (UNSUPPORTED.has(c)) return null;
    // Un comentario de Bash esconde comillas y escapes: no se puede seguir analizando.
    if (c === "#" && !word.started) return null;
    if (BLANKS.has(c)) {
      flush();
      i += 1;
    } else if (c === "\n" || c === ";" || c === "|" || (c === "&" && src[i + 1] !== ">")) {
      flush();
      tokens.push({ type: "separator" });
      i += separatorLength(src, i);
    } else if (c === ">" || c === "<" || c === "&") {
      const op = redirectOperator(src, i);
      if (op === null) return null;
      pushRedirect(op);
      i += op.length;
    } else if (readers[c]) {
      i = readers[c](src, i, word);
      if (i === FAIL) return null;
    } else {
      word.append(c, { bare: true });
      i += 1;
    }
  }
  flush();
  return tokens;
}

function assemble(tokens) {
  const commands = [];
  let current = { words: [], redirects: [] };

  const closeCommand = () => {
    if (current.words.length > 0 || current.redirects.length > 0) commands.push(current);
    current = { words: [], redirects: [] };
  };

  for (let k = 0; k < tokens.length; k += 1) {
    const token = tokens[k];
    if (token.type === "separator") {
      closeCommand();
    } else if (token.type === "word") {
      current.words.push(token.value);
    } else {
      const target = tokens[k + 1];
      if (target?.type !== "word") return null;
      current.redirects.push({ op: token.op, target: target.value });
      k += 1;
    }
  }
  closeCommand();
  return commands;
}

/**
 * Parte una orden de Bash en órdenes simples `{ words, redirects }`.
 * Devuelve null si contiene algo que no se puede analizar con certeza.
 */
export function parseCommandList(command) {
  if (command.includes("\r")) return null;
  const tokens = tokenize(command);
  return tokens === null ? null : assemble(tokens);
}

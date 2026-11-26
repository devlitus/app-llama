// Análisis sintáctico mínimo de una línea de shell, recorrida con estado de comillas.
// Solo acepta comandos simples unidos por tuberías: cualquier otra construcción devuelve null.
const FORBIDDEN_OUTSIDE_QUOTES = new Set([";", "&", ">", "<", "$", "`", "(", ")", "{", "}"]);
const FORBIDDEN_IN_DOUBLE_QUOTES = new Set(["$", "`"]);
const QUOTES = new Set(["'", '"']);

function isEscapedQuote(command, index) {
  return command[index] === "\\" && QUOTES.has(command[index + 1]);
}

// Devuelve los segmentos de la tubería (uno vacío no tiene primera palabra y lo rechaza la lista de lecturas), o null si la sintaxis no es la de un comando simple.
export function splitPipeline(command) {
  if (/[\r\n]/.test(command)) return null;
  const segments = [];
  let current = "";
  let quote = null;
  for (let i = 0; i < command.length; i += 1) {
    const char = command[i];
    if (quote === "'") {
      if (char === "'") quote = null;
    } else if (isEscapedQuote(command, i)) {
      return null;
    } else if (quote === '"') {
      if (FORBIDDEN_IN_DOUBLE_QUOTES.has(char)) return null;
      if (char === '"') quote = null;
    } else if (QUOTES.has(char)) {
      quote = char;
    } else if (FORBIDDEN_OUTSIDE_QUOTES.has(char)) {
      return null;
    } else if (char === "|") {
      segments.push(current);
      current = "";
      continue;
    }
    current += char;
  }
  segments.push(current);
  return quote === null ? segments.map((segment) => segment.trim()) : null;
}

const LIST_SEPARATORS = ["&&", "||", ";", "\n"];

// Parte una lista de órdenes por `&&`, `||`, `;` y saltos de línea fuera de comillas. Un `&` o un `|` sueltos se quedan en el elemento; cada elemento se valida después como tubería. Devuelve null con comillas escapadas o sin cerrar.
export function splitCommandList(command) {
  const items = [];
  let current = "";
  let quote = null;
  for (let i = 0; i < command.length; i += 1) {
    const char = command[i];
    if (quote === "'") {
      if (char === "'") quote = null;
    } else if (isEscapedQuote(command, i)) {
      return null;
    } else if (quote === '"') {
      if (char === '"') quote = null;
    } else if (QUOTES.has(char)) {
      quote = char;
    } else {
      const separator = LIST_SEPARATORS.find((candidate) => command.startsWith(candidate, i));
      if (separator) {
        items.push(current);
        current = "";
        i += separator.length - 1;
        continue;
      }
    }
    current += char;
  }
  items.push(current);
  return quote === null ? items.map((item) => item.trim()) : null;
}

// Palabras de un segmento ya validado, sin las comillas que las rodean.
export function wordsOf(segment) {
  const words = [];
  let current = "";
  let quote = null;
  for (const char of segment) {
    if (quote) {
      if (char === quote) quote = null;
      else current += char;
    } else if (QUOTES.has(char)) {
      quote = char;
    } else if (/\s/.test(char)) {
      if (current) words.push(current);
      current = "";
    } else {
      current += char;
    }
  }
  return current ? [...words, current] : words;
}

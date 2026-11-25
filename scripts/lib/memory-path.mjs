import { posix } from "node:path";

const MEMORY_DIR = "docs/memory";
const EXTENDED_PREFIX = /^\/\/[?.]\//;
const KEEP_SEGMENTS = [".", ".."];

// Windows ignora los puntos y espacios finales de un segmento: `memory.` es `memory`.
const stripTrailingDots = (segment) => (KEEP_SEGMENTS.includes(segment) ? segment : segment.replace(/[. ]+$/, ""));

function canonicalPath(path) {
  const slashed = path.replace(/\\/g, "/").replace(EXTENDED_PREFIX, "").toLowerCase();
  return posix.normalize(slashed.split("/").map(stripTrailingDots).join("/"));
}

function relativeToRoot(root, path) {
  const base = canonicalPath(root).replace(/\/$/, "");
  return path.startsWith(`${base}/`) ? path.slice(base.length + 1) : path;
}

// Una ruta es memoria si, vista desde la raíz del proyecto, es `docs/memory` o cuelga de ella.
export function isMemoryPath(root, filePath) {
  const rel = relativeToRoot(root, canonicalPath(filePath));
  return rel === MEMORY_DIR || rel.startsWith(`${MEMORY_DIR}/`);
}

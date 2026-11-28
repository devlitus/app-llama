import type { FailureKind } from "./types";

const KIB = 1024;
const UNITS = ["B", "KiB", "MiB", "GiB"] as const;
const NO_ETA = "—";

export function formatBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= KIB && unit < UNITS.length - 1) {
    value /= KIB;
    unit += 1;
  }
  const text = unit === 0 ? String(value) : value.toFixed(1).replace(".", ",");
  return `${text} ${UNITS[unit]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/s`;
}

export function formatEta(secs: number | null): string {
  if (secs === null) return NO_ETA;
  const hours = Math.floor(secs / 3600);
  const minutes = Math.floor((secs % 3600) / 60);
  const seconds = secs % 60;
  if (hours > 0) return joinParts([[hours, "h"], [minutes, "min"]]);
  if (minutes > 0) return joinParts([[minutes, "min"], [seconds, "s"]]);
  return `${seconds} s`;
}

// El segundo término se omite cuando vale 0 ("3 min", no "3 min 0 s").
function joinParts([[major, majorUnit], [minor, minorUnit]]: [[number, string], [number, string]]): string {
  const head = `${major} ${majorUnit}`;
  return minor === 0 ? head : `${head} ${minor} ${minorUnit}`;
}

export function failureMessage(kind: FailureKind): string {
  if (typeof kind === "string") return SIMPLE_MESSAGES[kind];
  if ("InsufficientSpace" in kind) {
    const { needed, available } = kind.InsufficientSpace;
    return `No hay espacio suficiente en disco: hacen falta ${formatBytes(needed)} y hay ${formatBytes(available)} libres.`;
  }
  if ("SizeMismatch" in kind) {
    const { expected, actual } = kind.SizeMismatch;
    return `El fichero descargado no tiene el tamaño esperado (${expected} B esperados, ${actual} B recibidos).`;
  }
  return `Error al escribir en disco: ${kind.Io.message}`;
}

const SIMPLE_MESSAGES: Record<Extract<FailureKind, string>, string> = {
  Network: "Error de red. Comprueba la conexión y vuelve a intentarlo.",
  Unauthorized: "Hace falta un token de Hugging Face válido. Configúralo en Ajustes.",
  Gated:
    "Este modelo requiere aceptar sus condiciones en la web del modelo antes de poder descargarlo.",
  NotFound: "El fichero ya no existe en Hugging Face.",
  ChecksumMismatch: "El fichero descargado está corrupto: su SHA256 no coincide.",
  UnexpectedContentRange: "El servidor respondió con un rango de datos incoherente.",
  DestinationExists: "Ya existe un fichero con ese nombre y distinto contenido en la carpeta de destino.",
};

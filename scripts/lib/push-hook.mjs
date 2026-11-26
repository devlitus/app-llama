// Veredicto R5 para el hook guard-shell: el analizador, con reserva y bloqueo ante un fallo.

const PUSH = /git\b[^;&|\n]*\bpush\b[^;&|\n]*/gi;
const FORCE = /(--force\b|--force-with-lease\b|\s-f\b|\s\+\S)/;
const PROTECTED = /\b(main|master)\b/i;
const UNRESOLVED = /[$`]/;

const deny = (reason) => ({ allow: false, reason });

// Reserva de último recurso, solo si falta el módulo del analizador: mira toda la orden, sin analizarla.
// Duplica a propósito `legacyPushVerdict` de push-guard.mjs: no puede importarlo, que es justo lo que falla.
function fallbackPushVerdict(command) {
  const plain = command.replaceAll("\\", "");
  const pushes = plain.match(PUSH);
  if (pushes === null) return { allow: true };
  if (FORCE.test(plain)) return deny("prohibido el push forzado. R5.");
  if (PROTECTED.test(plain)) return deny("prohibido hacer push directo a main/master; usa una rama y PR. R5.");
  if (pushes.some((push) => UNRESOLVED.test(push))) {
    return deny("no se puede analizar el push con certeza (variables o sustituciones). R5.");
  }
  return { allow: true };
}

async function loadGuardOrNull(loadGuard) {
  try {
    return await loadGuard();
  } catch {
    return null;
  }
}

/**
 * Un hook que lanza una excepción sale con 1, que no bloquea: ante un fallo, se deniega.
 * `loadGuard` importa el analizador; si falla, se usa la reserva.
 */
export async function pushVerdict(command, { tool, loadGuard, currentBranch }) {
  const guard = await loadGuardOrNull(loadGuard);
  try {
    return guard ? guard.checkPush(command, { tool, currentBranch }) : fallbackPushVerdict(command);
  } catch (error) {
    return deny(`no se pudo analizar el push (${error.message}); ante la duda, se bloquea. R5.`);
  }
}

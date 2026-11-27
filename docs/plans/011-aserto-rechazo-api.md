# 011 — Aserto exacto del rechazo en `api.test.ts`

- **Tipo:** chore
- **Estado:** hecho
- **Rama:** chore/011-aserto-rechazo-api
- **Fase del roadmap:** 0 (Esqueleto), mantenimiento de [docs/MVP.md](../MVP.md)

## Objetivo

Que el test que cubre el fallo de `getAppInfo()` solo pase si la UI recibe **el mismo error** que devolvió el backend. Así, si en el futuro alguien envuelve o sustituye el error, o el mock deja de estar activo, el test lo detecta.

## Contexto

### Qué hay hoy

- Origen: hallazgo menor del reviewer en la revisión final del plan 003 (T8.3). No se abrió issue.
- `ui/src/lib/api.ts` → `getAppInfo()` devuelve `invoke<AppInfo>("app_info")` tal cual, sin `catch` ni envoltorio.
- `ui/src/lib/api.test.ts:19-25`, en `get_app_info_propaga_error`, `mockIPC` lanza `new Error("ipc")` y el aserto es `await expect(getAppInfo()).rejects.toBeDefined()`. Pasa con **cualquier** rechazo cuyo valor no sea `undefined`.

### Cómo se propaga el error (verificado en `ui/node_modules/@tauri-apps/api` 2.12.5)

- `core.js:340`: `async function invoke(cmd, args = {}, options) { return window.__TAURI_INTERNALS__.invoke(cmd, args, options); }`.
- `mocks.js:128-133`: el `invoke` que instala `mockIPC` es `async` y hace `return cb(cmd, args)`. Si `cb` lanza, la promesa se rechaza con **ese mismo valor**: sin envolverlo, sin convertirlo a string y sin `transformCallback`.
- Por tanto, `getAppInfo()` se rechaza con la **misma instancia** de `Error` que crea el mock. Se puede afirmar por identidad.
- Caso que hoy pasa en falso: si no hay mock activo (por ejemplo, tras `clearMocks()`, que borra `__TAURI_INTERNALS__.invoke`), `invoke` se rechaza con un `TypeError` (`... invoke is not a function`). `rejects.toBeDefined()` también lo da por bueno.

### Decisiones técnicas de este plan

- **D1, aserto por identidad:** `const ipcError = new Error("ipc")`, el mock hace `throw ipcError` y el aserto es `await expect(getAppInfo()).rejects.toBe(ipcError)`. Se descarta `rejects.toThrow("ipc")`, porque compara por subcadena del mensaje: aceptaría un error distinto cuyo mensaje contenga `ipc` y no demuestra que `getAppInfo` no envuelva el error. La identidad es el contrato real de `api.ts`, que propaga el error sin transformarlo.
- **D2, nombre del test:** se mantiene `get_app_info_propaga_error`. El cambio solo refuerza el aserto, y así el histórico y el plan 003 siguen siendo trazables.
- **D3, demostración de que el aserto discrimina:** se hace con **mutaciones temporales** que no se versionan (CA2). Se descarta un meta-test permanente del tipo `expect(expect(...).rejects.toBe(...)).rejects.toThrow()`: prueba el framework de test, no el producto, y añade ruido.

### Auditoría del resto de tests de UI (`ui/src/**/*.test.ts`)

| Fichero | Asertos sobre promesas o errores | Conclusión |
|---|---|---|
| `lib/api.test.ts` | `resolves.toEqual({...})` (fuerte); `rejects.toBeDefined()` (débil) | Se corrige el segundo (T1) |
| `views/Settings.test.ts` | `findByText("Versión 0.1.0")` y `findByText("Versión no disponible")` | Correctos: verifican el resultado visible para el usuario, y Settings no muestra el error. El nombre del comando ya lo cubre `get_app_info_invoca_app_info`. Vitest falla ante rechazos sin capturar, así que "sin excepción sin capturar" queda cubierto |
| `App.test.ts` | Sin promesas ni errores | No aplica |
| `lib/navigation.test.ts` | Sin promesas ni errores | No aplica |

Búsqueda de `rejects|resolves|toBeDefined|toBeTruthy|toThrow|catch` en `ui/src`: solo dos coincidencias, las dos en `api.test.ts` (líneas 15 y 24). No hay más asertos de rechazo débiles.

## Criterios de aceptación

- **CA1 (aserto exacto)** — Dado `ui/src/lib/api.test.ts`, cuando se ejecuta `npm --prefix ui run test -- api.test`, entonces `get_app_info_propaga_error` pasa y su aserto es `rejects.toBe(ipcError)`, donde `ipcError` es la instancia que lanza el `mockIPC` del propio test.
- **CA2 (el aserto discrimina)** — Dado el test de CA1, cuando se aplican de una en una estas mutaciones temporales y se ejecuta `npm --prefix ui run test -- api.test`, entonces `get_app_info_propaga_error` **falla** en cada una (código de salida ≠ 0):
  - **M1:** el mock lanza `new Error("otro")` en lugar de `ipcError`.
  - **M2:** el mock lanza `new Error("ipc")`, una instancia nueva con el mismo mensaje que `ipcError`.
  - **M3:** se elimina la llamada a `mockIPC` del test, sin mock activo, y el rechazo es un `TypeError`.

  Después de revertir las mutaciones, `git diff` solo contiene el cambio de T1. El resultado de cada mutación (falla o pasa) se anota en T1.3.
- **CA3 (sin asertos débiles restantes)** — Dado `ui/src`, cuando se busca la regex `(rejects|resolves)\.toBeDefined\(\)` en `**/*.test.ts`, entonces hay 0 coincidencias.
- **CA4 (alcance)** — Dado el diff del plan, entonces solo cambian `ui/src/lib/api.test.ts` y este plan. No cambian `ui/src/lib/api.ts` ni ningún otro fichero de producción, y no aparece ningún patrón de R2.
- **CA5 (cierre)** — Dado el plan terminado, cuando se ejecutan `node scripts/memory.mjs check` y `node scripts/quality-gate.mjs`, entonces los dos terminan en verde, el reviewer da `APPROVED` y `docs/memory/medium/011-aserto-rechazo-api.md` queda en `estado: cerrado`.

## Tareas

> **(O)** = orchestrator. Commit sugerido para T1: `test(ui): aserto exacto del error en get_app_info_propaga_error`.

### T1 — Aserto por identidad en `get_app_info_propaga_error` (developer) ✅ hecha
- **Ficheros/módulos:** `ui/src/lib/api.test.ts`
- **Depende de:** —
- **Cubre:** CA1, CA2, CA3, CA4
- **Subtareas:**
  - [x] T1.1 (rojo) Antes de tocar el test, comprobar que hoy es débil: aplicar M1 sobre el test **actual** y ver que `rejects.toBeDefined()` **pasa**. Revertir.
  - [x] T1.2 (verde) En `get_app_info_propaga_error`: `const ipcError = new Error("ipc");`, `mockIPC(() => { throw ipcError; });` y `await expect(getAppInfo()).rejects.toBe(ipcError);`. Sin cambiar el nombre (D2) ni `api.ts`.
  - [x] T1.3 Ejecutar M1, M2 y M3 (CA2) de una en una, comprobar que el test falla en las tres y revertir. Anotar el resultado en este plan, debajo de esta subtarea, con una línea por mutación, y en la entrada de sesión de memoria del developer.
    - T1.1: M1 sobre el test anterior (`toBeDefined`) pasa (salida 0): el aserto era débil.
    - M1 (`new Error("otro")`): el test falla (salida 1).
    - M2 (`new Error("ipc")`, otra instancia): el test falla (salida 1).
    - M3 (sin `mockIPC`, `TypeError`): el test falla (salida 1).
  - [x] T1.4 Ejecutar la búsqueda de CA3 y `node scripts/quality-gate.mjs` en verde.
- **Tests unitarios** (`ui/src/lib/api.test.ts`, Vitest, sin red: solo `mockIPC`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `get_app_info_invoca_app_info` (sin cambios) | `mockIPC` que guarda `cmd` y responde `{name:"app-llama",version:"0.1.0"}` | resuelve ese objeto; `commands == ["app_info"]` |
  | `get_app_info_propaga_error` (modificado) | `mockIPC` que lanza `ipcError = new Error("ipc")` | `rejects.toBe(ipcError)`: la misma instancia |
  | M1 (mutación temporal, no se versiona) | el mock lanza `new Error("otro")` | `get_app_info_propaga_error` falla |
  | M2 (mutación temporal, no se versiona) | el mock lanza otra instancia `new Error("ipc")` | `get_app_info_propaga_error` falla |
  | M3 (mutación temporal, no se versiona) | sin `mockIPC` en el test (rechazo `TypeError`) | `get_app_info_propaga_error` falla |

### T2 — Revisión final y cierre (O + reviewer) ✅ hecha
- **Depende de:** T1
- **Cubre:** CA4, CA5, y todos en la revisión
- **Subtareas:**
  - [x] T2.1 El orchestrator lanza el `reviewer` con este plan, el diff completo de la rama contra `main` y `última tarea: sí`. El reviewer repite al menos M1 para comprobar CA2 y hace `session`, `flush` y, con `APPROVED`, `close`. Como máximo, 3 ciclos.
  - [ ] T2.2 (O) `node scripts/memory.mjs check` y `node scripts/quality-gate.mjs` en verde → `Estado: hecho`. Se anota el resultado en este plan.
    - **Resultado (2026-11-27):** reviewer `APPROVED` en el ciclo 1, sin hallazgos ni issues; repitió M1 y M2 (ambas fallan). Memoria `011-aserto-rechazo-api` cerrada; check de memoria (4 planes, 4 cerrados) y quality gate en verde.
- **Tests unitarios:** no aplica (los automatizados son los de T1).

## Fuera de alcance

- Cambiar `ui/src/lib/api.ts` o introducir un tipo de error propio para el IPC. El contrato sigue siendo "propagar el error sin transformarlo".
- Tests de `Settings.test.ts`: según la auditoría del Contexto, sus asertos son correctos para lo que verifican.
- `click_en_la_activa_no_cambia` (`App.test.ts`) solo comprueba el `h1` y no que siga habiendo un único `aria-current="page"`. No es un aserto de rechazo y no entra en este plan. Si se quiere reforzar, irá en su propio plan.
- Mutation testing automatizado (Stryker u otros) en el gate.
- Forma real de los errores de Tauri en producción (string u objeto serializado desde Rust). Se tratará cuando haya comandos que devuelvan `Result` (fase 1 en adelante).

## Riesgos

| Riesgo | Mitigación |
|---|---|
| Una versión futura de `@tauri-apps/api` cambia `mockIPC` y envuelve el error, con lo que `toBe` falla aunque `api.ts` sea correcto | La versión queda fijada por `ui/package-lock.json` (R3). Si ocurre al actualizar, el fallo es visible y se ajusta el aserto en el plan de la actualización, revisando `mocks.js` de nuevo. |
| Una mutación temporal se queda en el commit | CA2 y CA4 exigen que `git diff` solo contenga el cambio de T1. El reviewer revisa el diff completo (T2.1). |
| `npm --prefix ui run test -- api.test` filtra más ficheros de los esperados | El filtro solo afecta a la verificación de CA1 y CA2. El gate ejecuta toda la suite igualmente (T1.4, T2.2). |

## Preguntas abiertas

- Ninguna.

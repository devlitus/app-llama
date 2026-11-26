---
schema: memoria-plan/2
plan: 003-esqueleto
estado: abierto
---

# Memoria del plan 003-esqueleto

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a9449de74b77d94dc | rojo | APPROVED |

### Resúmenes

- C1 · developer · T1 escrita (workspace, toolchain, crate core, tests TDD) pero sin ejecutar: no hay Rust/cargo en la maquina, gate y Cargo.lock pendientes
- C1 · reviewer · T1 verificada por el reviewer, workspace con solo app-llama-core, 6 modulos con comentario de modulo, about.rs con 4 tests en verde y gate verde con rustc 1.100.0; la sesion developer-1 (sin ejecutar, gate rojo, reserva-sin-test) esta obsoleta y no refleja el estado real

### Decisiones

- **limitacion-documentada** — memory.mjs no admite una segunda sesion con el mismo agent_id; el estado real de T1 lo certifica esta entrada del reviewer, no la del developer (C1)

### Errores

- **reserva-sin-test** — Rust no instalado: no se pudo validar rojo/verde ni generar Cargo.lock; requiere decision del usuario para instalar rustup (C1)

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | ae91771dfbf0346e4 | rojo | APPROVED |

### Resúmenes

- C1 · developer · T2 hecha: planSteps puro en gate-steps.mjs con 7 tests, quality-gate lo usa, fila Build UI en GOVERNANCE §2
- C1 · reviewer · planSteps puro con orden CA6, comandos Rust y etiquetas intactos, fila Build UI en GOVERNANCE; gate verde

### Decisiones

- **limitacion-documentada** — Build UI se planifica primero porque src-tauri incrusta ui/dist; nota en GOVERNANCE §2 (C1)
- **refactor-equivalente** — la extracción conserva salida, mensajes de error y códigos de salida de quality-gate.mjs (C1)

### Errores

- (ninguno)

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | afcc2cac9cfd8dd09 | rojo | APPROVED |

### Resúmenes

- C1 · developer · Proyecto ui/ con Vite 8, Svelte 5, tooling (eslint, svelte-check, vitest) y navigation.ts con 6 tests; gate verde.
- C1 · reviewer · T3 aprobada; proyecto UI Vite+Svelte5+TS con 6 tests de navegacion, gate verde y npm ci sin tocar el lockfile

### Decisiones

- **limitacion-documentada** — resolve.conditions browser solo con mode test en vite.config (sin process, evita @types/node). (C1)
- **refactor-equivalente** — Se aceptan test-setup.ts, svelte.config.js, resolve.conditions solo en modo test y throw en main.ts como desviaciones equivalentes o mejores que el plan (C1)

### Errores

- **codigo-muerto** — sectionLabel tiene una rama de reserva inalcanzable (devuelve id) y globals.node se aplica tambien a src/ (minor, sin issue) (C1)

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a6ccea229703c3d6f | rojo | APPROVED |

### Resúmenes

- C1 · developer · T4 shell de la UI con navegación y 5 vistas; corregidos hallazgos de T3 (sectionLabel sin rama muerta, globals de eslint por ámbito)
- C1 · reviewer · Shell de la UI con 5 botones, h1 por vista y aria-current solo en la activa; CA4 cubierto por App.test.ts y minors de T3 corregidos; gate verde

### Decisiones

- **limitacion-documentada** — cleanup de testing-library registrado en test-setup porque vitest no usa globals (C1)
- **refactor-equivalente** — sectionLabel pasa a un mapa LABELS derivado de SECTIONS, sin rama muerta y mismo contrato (C1)

### Errores

- (ninguno)

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a733b59b2801228ea | rojo | APPROVED |

### Resúmenes

- C1 · developer · T5 hecha: app Tauri 2.12.7 con comando app_info, 3 tests IPC en verde, gate verde y tauri build --debug --no-bundle verificado en local
- C1 · reviewer · T5 app Tauri y comando app_info correctos; gate verde, CA1/CA3/CA9 verificados y manifiesto incrustado en el exe

### Decisiones

- **limitacion-documentada** — csp null aceptado en el esqueleto como minor; la CSP estricta queda para el plan 009 (C1)
- **manifiesto-cargo-manifest-dir** — build.rs usa CARGO_MANIFEST_DIR para /MANIFESTINPUT y try_build sin unwrap; la ruta es absoluta y robusta (C1)
- **refactor-equivalente** — build.rs usa manifiesto propio (Common Controls v6) con rustc-link-arg y new_without_app_manifest para que arranquen los tests de la lib en Windows (C1)

### Errores

- (ninguno)

## T6

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a2d980c8054de2b22 | rojo | APPROVED |

### Resúmenes

- C1 · developer · T6 versión en Ajustes vía getAppInfo/app_info y hallazgos menores de T5 (CSP, tauri, build.rs, tildes)
- C1 · reviewer · T6 aprobada; api.ts único punto con app_info, Ajustes muestra versión o no disponible, gate verde, minors de T5 corregidos salvo una tilde

### Decisiones

- **limitacion-documentada** — CSP con style-src unsafe-inline e img-src data: sin uso actual en la UI; se acepta como minor (C1)

### Errores

- **csp-permisiva** — img-src data: y unsafe-inline añadidos sin necesidad verificada; queda "json valido" sin tilde en lib.rs:75; rojo de TDD no ejecutado (proceso) (C1)

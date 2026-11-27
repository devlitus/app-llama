---
schema: memoria-plan/2
plan: 003-esqueleto
estado: cerrado
---

# Memoria del plan 003-esqueleto

## Patrones del plan

### Decisiones

- **csp-estricta** — Se quita unsafe-inline y data: de la CSP; build debug compila y el gate está en verde. (T7)
- **csp-estricta-sin-devcsp** — La CSP solo se aplica a assets incrustados (protocolo tauri); con devUrl la sirve Vite y no se aplica, asi que no hace falta devCsp (T7)
- **limitacion-documentada** — CA8 verificado en vivo solo en Windows y macOS solo por CI (run 45953175348), segun MVP 7.4 (T1, T2, T3, T4, T5, T6, T7, T8)
- **manifiesto-cargo-manifest-dir** — build.rs usa CARGO_MANIFEST_DIR para /MANIFESTINPUT y try_build sin unwrap; la ruta es absoluta y robusta (T5)
- **origen-local-por-plataforma** — el origen local de los tests IPC replica protocol::origin de tauri (http://tauri.localhost en Windows/Android, tauri://localhost en el resto) (T5)
- **refactor-equivalente** — beforeDevCommand/beforeBuildCommand con la forma script+cwd prevista en Riesgos en lugar de npm --prefix (T2, T3, T4, T5, T7, T8)

### Errores

- **aserto-debil** — api.test.ts usa rejects.toBeDefined y el texto de T5.3 no refleja la forma script+cwd (minor, sin issue) (T8)
- **codigo-muerto** — sectionLabel tiene una rama de reserva inalcanzable (devuelve id) y globals.node se aplica tambien a src/ (minor, sin issue) (T3)
- **csp-permisiva** — img-src data: y unsafe-inline añadidos sin necesidad verificada; queda "json valido" sin tilde en lib.rs:75; rojo de TDD no ejecutado (proceso) (T6)
- **reserva-sin-test** — Rust no instalado: no se pudo validar rojo/verde ni generar Cargo.lock; requiere decision del usuario para instalar rustup (T1)
- **test-origen-no-portable** — El helper de test fijaba http://tauri.localhost, remoto en macOS/Linux, y la ACL rechazaba app_info (CI PR #26). (T5 · #26)

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a9449de74b77d94dc | rojo | APPROVED |

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | ae91771dfbf0346e4 | rojo | APPROVED |

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | afcc2cac9cfd8dd09 | rojo | APPROVED |

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a6ccea229703c3d6f | rojo | APPROVED |

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a733b59b2801228ea | rojo | APPROVED |
| 2 | 2026-11-27 | 1 | a8c30b05b39999d63 | verde | APPROVED |

## T6

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a2d980c8054de2b22 | rojo | APPROVED |

## T7

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 2 | ac4ec17a4c69c0d86 | verde | APPROVED |

## T8

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 0 | adc52d3fe135bbfb6 | verde | APPROVED |

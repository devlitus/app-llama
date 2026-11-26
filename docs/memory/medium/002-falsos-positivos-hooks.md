---
schema: memoria-plan/2
plan: 002-falsos-positivos-hooks
estado: cerrado
---

# Memoria del plan 002-falsos-positivos-hooks

## Patrones del plan

### Decisiones

- **analizador-null-ante-duda** — splitCommandList devuelve null con comillas escapadas o sin cerrar y la lista blanca sigue cerrada. (T2, T3, T4, T5)
- **deuda-aceptada-usuario** — T4 se aprueba con deuda #22 (alias con prefijo) y #24 (--config-env relajado) por decisión explícita del usuario en la escalada (T4)
- **limitacion-documentada** — El rg literal de CA11 también casa con isProtectedRef/isProtectedBranch de push-guard y con la memoria media; con -w solo da el encabezado de R8, se acepta como imprecisión del plan (T1, T2, T3, T4, T5, T7)

### Errores

- **comentario-oculta-orden** — un comentario con comillas escondía una orden; corregido, issue #15 (T2 · #15)
- **config-en-linea** — la relajación de -c alcanza a --config-env, cuyo valor viene del entorno y nunca se menciona (#24) (T4 · #21, #22, #23, #24)
- **glob-sin-comillas** — comodines sin comillas llegaban literales; corregido, issue #16 (T2 · #16)
- **opcion-abreviada** — prefijos de --all/--mirror/--branches/--for* tratados como la opcion (#18); rojo previo: 4 tests de push-guard fallaban y push-hook.test no cargaba (T4 · #18)
- **orden-anidada** — un heredoc cat a fichero seguido de sh fichero en la misma orden pasa tras el despojo; antes se negaba, aunque Write+sh ya lo permitia (T4, T5 · #20, #21, #22)
- **refspec-dwim** — heads/<rama> y remotes/o/<rama> normalizados como refs/heads (#19) (T4 · #19)
- **reserva-sin-test** — pushVerdict en scripts/lib/push-hook.mjs con tests de excepcion y reserva (T1, T4)
- **rg-ca11-subcadena** — El patrón isProtected de CA11 casa por subcadena con nombres nuevos de push-guard.mjs; usar -w en futuros greps de retirada (T7)
- **tuberia-tras-cabecera** — Heredoc con cabecera terminada en | (también |& y 2>&1 |) se despojaba y ocultaba un cuerpo ejecutable; #17. (T3 · #17)

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | ae406e9e243cbf440 | verde | APPROVED |

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a04e0f2e70405583f | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | a66e47dab2274d979 | verde | APPROVED |

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a3207262c8c2205fa | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | a1ba82b14b87c5611 | verde | APPROVED |

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | ac66b75c86e420c2f | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | a0b0931e5e80d3d1f | verde | CHANGES_REQUESTED |
| 3 | 2026-11-26 | 1 | ac93d0e14be38532d | verde | CHANGES_REQUESTED |
| 4 | 2026-11-26 | 1 | a4921988fa5f95ab8 | verde | CHANGES_REQUESTED |
| 5 | 2026-11-26 | 0 | aa88716280880b1d8 | verde | APPROVED |

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | ad93d98d9bf1d2ac8 | verde | APPROVED |

## T7

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 0 | aa88716280880b1d8 | verde | APPROVED |

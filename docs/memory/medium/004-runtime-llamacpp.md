---
schema: memoria-plan/2
plan: 004-runtime-llamacpp
estado: abierto
---

# Memoria del plan 004-runtime-llamacpp

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a7c554070d131c111 | verde | APPROVED |

### Resúmenes

- C1 · developer · T1 plan 004: modelo Release/Asset, parse_releases, build_number, sha256, latest_build_release y RuntimeError inicial, con fixtures.
- C1 · reviewer · T1 aprobada; gate verde, CA4 y los 9 tests del plan presentes; solo hallazgos minor

### Decisiones

- **refactor-equivalente** — se acepta decodificar hex a mano sin el crate hex hasta T5 y mod release privado con re-exports (C1)

### Errores

- **validacion-hex-laxa** — decode_sha256_hex acepta '+' porque u8::from_str_radix admite signo; minor, falla en lado seguro (C1)

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | aa060e1b875ac7db1 | verde | APPROVED |

### Resúmenes

- C1 · developer · T2 clasificación de assets por tokens (asset.rs) y ReleaseParse sin duplicar el error de serde
- C1 · reviewer · classify_asset por tokens cumple el Diseño y los 8 tests del plan; gate verde; solo hallazgos minor

### Decisiones

- **limitacion-documentada** — la sensibilidad a mayúsculas y la doble tolerancia de "cu" se aceptan como minor sin issue (C1)
- **refactor-equivalente** — ReleaseParse mantiene #[source] y el mensaje ya no incluye {0} (C1)

### Errores

- **aserto-debil** — clasificacion_sensible_a_mayusculas falla por la extensión .ZIP antes de mirar los tokens; no prueba lo que dice (C1)
- **reserva-sin-test** — Escribí tests y código a la vez, sin comprobar antes el rojo del TDD (C1)
- **validacion-hex-laxa** — el brazo [version] quita "cu" dos veces: "win-cucu12.4-x64.zip" se clasifica como Cuda{12,4} (C1)

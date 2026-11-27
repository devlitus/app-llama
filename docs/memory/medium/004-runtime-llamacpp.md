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

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | af1acb609ece412c4 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 0 | a8395a4fa2e892007 | verde | APPROVED |

### Resúmenes

- C1 · developer · T3 detección de hardware (SystemProbe, detect, parsers nvidia-smi, FakeProbe) y correcciones de T2 en asset.rs
- C1 · reviewer · deteccion de hardware correcta y CA1 cubierto; NativeProbe::run lanza nvidia-smi sin CREATE_NO_WINDOW en una app GUI
- C2 · reviewer · ciclo 2 de T3 correcto; hide_console, run_with_timeout de 10 s, supertrait Send+Sync y run_count tolerante a envenenamiento resuelven #28 y los minors

### Decisiones

- **limitacion-documentada** — nieto que herede stdout y aserto elapsed<20 s aceptados como minor por el orchestrator (C1, C2)
- **refactor-equivalente** — prefijo cu de CUDA se quita solo en el brazo de parse_backend, parse_cuda_version recibe la versión limpia (C1)

### Errores

- **aserto-debil** — FakeProbe::run descarta el registro si el mutex está envenenado mientras run_count lo recupera; incoherencia menor (C1, C2)
- **consola-visible-windows** — NativeProbe::run sin CREATE_NO_WINDOW hace parpadear consolas en la release de Windows #28 (C1 · #28)

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | abb1afe09c37e59fd | verde | APPROVED |

### Resúmenes

- C1 · developer · T4 select.rs (BackendPreference, InstallPlan, available_backends, select_install, is_compatible) con 16 tests; fixture b12780 ampliado y FakeProbe::run sin perder llamadas con mutex envenenado.
- C1 · reviewer · select.rs cubre CA2/CA3 con los 16 tests de la tabla; gate verde; solo minors

### Decisiones

- **cpu-mayor-isa** — CPU elige la mayor ISA soportada con Generic como minimo: el Diseño contradice su propia tabla de tests y prevalecen los tests (C1)
- **cuda-por-driver** — CUDA basta con driver_cuda (solo lo produce nvidia-smi) y compara solo major, segun el Diseño (C1)
- **limitacion-documentada** — CPU elige la mayor ISA soportada con Generic como mínimo (el plan decía preferir Generic, que contradice legacy_cpu_mejor_isa); CpuIsa deriva Ord. (C1)

### Errores

- **rama-inalcanzable** — cuda_version tiene un brazo _ => (0,0) inalcanzable; mejor llevar (major, minor) en el filter_map (minor) (C1)
- **reserva-sin-test** — BackendPreference deriva Deserialize con rename_all lowercase sin test que lo fije (minor) (C1)

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a99603e7b78627d21 | verde | APPROVED |

### Resúmenes

- C1 · developer · T5 descargador simple (fetch.rs, CancelToken, SimpleHttpFetcher) con 9 tests wiremock; hallazgos de T1 y T4 corregidos
- C1 · reviewer · T5 C1 descargador simple: CA5 cubierto y gate verde; solo minors (tamaño sin corte temprano, cancelación a mitad sin test, part_of duplicado)

### Decisiones

- **limitacion-documentada** — tamaño comprobado al final y cancelación entre bloques aceptados como minor; 006 aportará el descargador robusto (C1)
- **refactor-equivalente** — cuda_version eliminado: el filter_map devuelve ((major, minor), candidate) y max_by_key usa esa tupla (C1)

### Errores

- **reserva-sin-test** — la rama de cancelación con .part ya creado (fetch.rs:81) no tiene test; solo se prueba la cancelación previa a la petición (C1)
- **validacion-hex-laxa** — Asset::sha256 aceptaba "+f" repetido con from_str_radix; ahora hex::decode_to_slice con test de regresión (C1)

## T6

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a2af751013ba9b0e9 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | a85a928aa5d3ad242 | verde | APPROVED |

### Resúmenes

- C1 · developer · T6 extract.rs (zip/tar.gz seguro + find_server_binary) y hallazgos de T5 en fetch.rs (límite de tamaño en streaming, Content-Length, cancelación)
- C1 · reviewer · extracción zip/tar.gz y find_server_binary correctas salvo que la comprobación de symlinks del tar.gz se elude; gate verde
- C2 · developer · T6 ciclo 2: extracción tar.gz endurecida (destino de enlaces sin .., ./ ignorado, sin entradas bajo symlinks), find_server solo ficheros regulares, modos enmascarados.
- C2 · reviewer · T6 C2 aprobada; el escape por symlinks de #29 queda cerrado (verificado con PoC) y el gate está en verde

### Decisiones

- **limitacion-documentada** — los tests de symlink que dependen de privilegios solo corren en Unix; la comprobación de ancestros es una defensa en profundidad sin test propio (minor) (C1, C2)
- **refactor-equivalente** — min_by sin clonar y set_mask(0o7000) en tar, & 0o777 en zip. (C1, C2)

### Errores

- **reserva-sin-test** — los tests de ./l y encadenado fallan por el destino '..' y no ejercitan has_symlink_ancestor ni normal_components (extract.rs:109-111, 137-148) (C2)
- **symlink-forma-eludible** — validar symlinks por la forma de la ruta se eludía con ./ y encadenado; ahora se rechaza el destino con .. y se rastrean symlinks (#29). (C1, C2 · #29)

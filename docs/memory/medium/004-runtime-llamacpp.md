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

## T7

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a514f403df71c417f | verde | APPROVED |

### Resúmenes

- C1 · developer · T7 verify.rs (parse_version, CommandRunner, TokioCommandRunner, verify_server, remove_quarantine), FakeRunner y tests de extract del hallazgo de T6
- C1 · reviewer · T7 C1 aprobada; verify_server, remove_quarantine, TokioCommandRunner y FakeRunner cubren los 14 tests del plan, gate verde

### Decisiones

- **consola-oculta-compartida** — hide_console_async en process.rs reutiliza CREATE_NO_WINDOW; CommandOutput lleva exit_code Option<i32> (C1)
- **limitacion-documentada** — remove_quarantine rechaza rutas no UTF-8 con QuarantineFailed; aceptable porque APFS exige nombres UTF-8 (C1)
- **refactor-equivalente** — CommandRunner Send+Sync y &dyn CommandRunner aceptados: necesarios para Arc<dyn> en T10 (C1)

### Errores

- **ruta-relativa-con-cwd** — verify_server usa cwd=parent del binario; con ruta relativa en Unix el programa se resuelve tras chdir y falla (minor, produccion usa rutas absolutas) (C1)

## T8

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a8d296d0c00cff5d2 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | a63b097742c4ffbdc | verde | APPROVED |

### Resúmenes

- C1 · developer · T8 estado persistido del runtime (state.rs) con StateStore atómico y status_of; Serialize/Deserialize en Backend, CpuIsa, RuntimeVersion
- C1 · reviewer · T8 estado persistido; gate verde y 10 tests del plan, pero load() trata la lectura no UTF-8 y otros errores de E/S como inexistente, sin .bak (CA11)
- C2 · developer · T8 ciclo 2: load() devuelve Result, lee bytes (no UTF-8 cuenta como corrupto) y propaga errores de E/S; tests nuevos.
- C2 · reviewer · T8 C2 aprobada; load() devuelve Result, no UTF-8 hace .bak y errores de E/S no se enmascaran (#30 resuelto)

### Decisiones

- **limitacion-documentada** — StateStore documenta que no es seguro para escrituras concurrentes; la serialización queda para T10 (C1, C2)
- **refactor-equivalente** — load() devuelve Default ante corrupto/schema desconocido y copia a .bak best-effort (C1)

### Errores

- **corrupto-sin-copia** — load() con bytes no UTF-8 devolvía Default sin .bak (#30). (C1, C2 · #30)
- **reserva-sin-test** — save sobre un runtime.json existente (reemplazo en Windows) sin test (C1)

## T9

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a554b9563c55efccf | verde | APPROVED |

### Resúmenes

- C1 · developer · T9 cliente de releases de GitHub (ReleaseSource, GitHubReleaseSource) con 7 tests wiremock.
- C1 · reviewer · T9 C1 cliente de releases de GitHub con wiremock; 7 tests del plan presentes, gate verde, solo hallazgos minor

### Decisiones

- **analizador-null-ante-duda** — x-ratelimit-reset no numérico, negativo o decimal se degrada a reset_epoch None sin error; correcto, sin test propio (C1)
- **limitacion-documentada** — base_url con barra final produce //repos; se acepta porque producción inyecta https://api.github.com sin barra, recomendado normalizar en new() (C1)
- **refactor-equivalente** — per_page literal en la URL en vez de .query() por la feature de reqwest 0.13; equivalente y cubierto por query_param en el mock (C1)

### Errores

- **reserva-sin-test** — rama de x-ratelimit-reset no numérico sin test que fije el None (minor) (C1)

## T10

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a71e1a65cc61af1b2 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | a78564bc4f0f6876f | verde | APPROVED |

### Resúmenes

- C1 · developer · RuntimeManager (status, hardware, check_update, use_external) con cerrojo de estado y bloqueo de instalacion; minors de github.rs
- C1 · reviewer · T10 C1, gate verde y CA9/CA10/CA11 con tests; 2 major: backend inventado para externo y pending_cleanup con NotFound
- C2 · developer · T10 ciclo 2: backend Option, NotFound como exito, borrado en spawn_blocking, nombre por Os, external rechazado si instalando
- C2 · reviewer · T10 C2 gate verde; #31 y #32 resueltos con tests; CA9/CA10/CA11 cubiertos; solo minors de nomenclatura y cobertura de active_server_path

### Decisiones

- **backend-externo-opcional** — Option<Backend> con serde(default) lee runtime.json previos como Some y externos/null como None; verificado con tests (C1, C2)
- **limitacion-documentada** — active_server_path async con Result aceptado (status puede fallar por E/S); hay que actualizar el plan para 008 (C1)
- **refactor-equivalente** — remove_dirs async con spawn_blocking y NotFound como borrado, reutilizable por T11 (C1, C2)

### Errores

- **dato-inventado** — use_external persistia Cpu Generic para binarios externos; ahora None (#31) (C1, C2 · #31)
- **notfound-no-es-exito** — retry_pending_cleanup dejaba rutas inexistentes pendientes para siempre; ahora NotFound cuenta como borrado (#32) (C1, C2 · #32)
- **reserva-sin-test** — active_server_path sin test con Broken -> Ok(None) ni con error de E/S -> Err; nombres de test del plan distintos del código (minor) (C2)

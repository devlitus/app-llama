---
schema: memoria-plan/2
plan: 004-runtime-llamacpp
estado: cerrado
---

# Memoria del plan 004-runtime-llamacpp

## Patrones del plan

### Decisiones

- **analizador-null-ante-duda** — x-ratelimit-reset no numérico, negativo o decimal se degrada a reset_epoch None sin error; correcto, sin test propio (T9)
- **backend-externo-opcional** — Option<Backend> con serde(default) lee runtime.json previos como Some y externos/null como None; verificado con tests (T10)
- **consola-oculta-compartida** — hide_console_async en process.rs reutiliza CREATE_NO_WINDOW; CommandOutput lleva exit_code Option<i32> (T7)
- **cpu-mayor-isa** — CPU elige la mayor ISA soportada con Generic como minimo: el Diseño contradice su propia tabla de tests y prevalecen los tests (T4)
- **csp-estricta** — progreso con <progress> sin style en línea; JSON de Rust fijado en un test para que los tipos TS no diverjan (T12, T13)
- **cuda-por-driver** — CUDA basta con driver_cuda (solo lo produce nvidia-smi) y compara solo major, segun el Diseño (T4)
- **directorio-por-tag-y-backend** — free_destination saca final_dir de pending_cleanup tras clear_residual y antes del rename; si el rename falla no se pierde nada porque el destino ya no existe (T11, T12)
- **formato-version-dual** — la cabecera de nvidia-smi se acepta en formato antiguo (CUDA Version) y nuevo (CUDA UMD Version) en la misma linea (T14)
- **limitacion-documentada** — los minors de T14 (condición hint duplicada, fixture del aviso distinto del plan) y el parser CUDA por primera línea quedan documentados sin corregir (T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T13, T14, T15)
- **limpieza-best-effort** — el token de cancelación se libera tras install con éxito o error; pánico queda como minor (guarda RAII recomendada) (T11, T12)
- **refactor-equivalente** — la acción se decide por kind y no por la etiqueta; busy común deshabilita selector, primario y binario externo (T1, T2, T3, T5, T6, T7, T8, T9, T10, T11, T13)

### Errores

- **acoplamiento-por-etiqueta** — El componente comparaba action.label con una constante; corregido con kind. (T13)
- **aserto-debil** — escapes de ruta Windows rotos al generar tests con script; el lint los detectó (T2, T3, T11, T13, T14)
- **carrera-pending-cleanup** — status() entre rename y modify_state borraba el runtime recien instalado; corregido con free_destination previo al rename (T11, T12)
- **condicion-sin-test** — la condición extra !installing que oculta el aviso durante la instalación está en el .svelte, duplicada, sin test ni reflejo en T13.5 (minor) (T14)
- **consola-visible-windows** — NativeProbe::run sin CREATE_NO_WINDOW hace parpadear consolas en la release de Windows #28 (T3 · #28)
- **corrupto-sin-copia** — load() con bytes no UTF-8 devolvía Default sin .bak (#30). (T8 · #30)
- **dato-inventado** — el parser solo conocia la etiqueta antigua; el formato de nvidia-smi cambia entre drivers, validar con fixtures reales (T10, T14 · #31)
- **exito-reportado-como-error** — si falla el listado del paso 9, install devuelve Err con el runtime nuevo ya activo (T11)
- **fixture-distinto-del-plan** — runtime_settings_aviso_backends usa External b12000 e installed null en vez de Managed b12816 Cpu de la tabla del plan (T14, T15)
- **notfound-no-es-exito** — retry_pending_cleanup dejaba rutas inexistentes pendientes para siempre; ahora NotFound cuenta como borrado (#32) (T10 · #32)
- **plan-desalineado** — el paso 8 del plan dice que final_dir sale de pending_cleanup en la misma escritura; ahora ocurre antes del rename (minor) (T12)
- **rama-inalcanzable** — cuda_version tiene un brazo _ => (0,0) inalcanzable; mejor llevar (major, minor) en el filter_map (minor) (T4)
- **rename-sobre-existente** — rename de staging a runtime/<tag> falla con Io al reinstalar el mismo tag con otro backend #33 (T11 · #33)
- **reserva-sin-test** — CA12 Cancelar y reseteo de installing/unlisten sin test de componente; se dio por difícil pero mockIPC(..., { shouldMockEvents: true }) de @tauri-apps/api 2.12.5 lo permite (T2, T4, T5, T6, T8, T9, T10, T11, T13 · #33)
- **ruta-relativa-con-cwd** — verify_server usa cwd=parent del binario; con ruta relativa en Unix el programa se resuelve tras chdir y falla (minor, produccion usa rutas absolutas) (T7)
- **staging-reutilizado** — si no se pudo borrar .staging-<tag> huérfano, la extracción mezcla contenido viejo y nuevo (T11)
- **symlink-forma-eludible** — validar symlinks por la forma de la ruta se eludía con ./ y encadenado; ahora se rechaza el destino con .. y se rastrean symlinks (#29). (T6 · #29)
- **token-sin-guarda-raii** — finish_install no se ejecuta si install entra en pánico: runtime_install respondería AlreadyInstalling hasta reiniciar (minor) (T12)
- **validacion-hex-laxa** — Asset::sha256 aceptaba "+f" repetido con from_str_radix; ahora hex::decode_to_slice con test de regresión (T1, T2, T5)

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a7c554070d131c111 | verde | APPROVED |

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | aa060e1b875ac7db1 | verde | APPROVED |

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | af1acb609ece412c4 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 0 | a8395a4fa2e892007 | verde | APPROVED |

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | abb1afe09c37e59fd | verde | APPROVED |

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a99603e7b78627d21 | verde | APPROVED |

## T6

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a2af751013ba9b0e9 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | a85a928aa5d3ad242 | verde | APPROVED |

## T7

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a514f403df71c417f | verde | APPROVED |

## T8

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a8d296d0c00cff5d2 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | a63b097742c4ffbdc | verde | APPROVED |

## T9

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a554b9563c55efccf | verde | APPROVED |

## T10

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a71e1a65cc61af1b2 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | a78564bc4f0f6876f | verde | APPROVED |

## T11

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a198b00b7220a89af | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | a981f48ba5fe195a8 | verde | APPROVED |

## T12

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | ab22f887797c7f532 | verde | APPROVED |

## T13

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a1b2ee2854a65c3d3 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-27 | 1 | ab6aee5c410a1fdbb | verde | APPROVED |

## T14

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a7d69c8e7e19f946f | verde | APPROVED |
| 2 | 2026-11-27 | 1 | a03de7d42f04eeba9 | verde | APPROVED |
| 3 | 2026-11-27 | 1 | a2aea020c5ccc0b77 | verde | APPROVED |

## T15

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 0 | a4b6f67fb81579c4c | verde | APPROVED |

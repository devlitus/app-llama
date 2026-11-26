# 004 — Runtime de llama.cpp: detección de hardware, instalación y actualización

- **Tipo:** feature
- **Estado:** borrador
- **Rama:** feat/004-runtime-llamacpp
- **Fase del roadmap:** Fase 1 (Runtime) de [docs/MVP.md](../MVP.md), funcionalidad A

## Objetivo

Que el usuario tenga un `llama-server` funcional sin compilar nada. La app detecta su hardware, descarga e instala la build precompilada de llama.cpp que mejor le encaja (CUDA, Vulkan, Metal o CPU) y la verifica. También puede elegir un binario que ya tenga instalado. Desde Ajustes ve qué versión usa y la actualiza con un clic.

## Contexto

### Punto de partida

- Se supone hecho el plan 003 (esqueleto): workspace Cargo con `crates/core` (módulos vacíos, entre ellos `runtime`), `src-tauri/` (Tauri 2), `ui/` (Svelte 5 + TS, con `lint`, `check` y `test` con Vitest), CI Windows + macOS y quality gate. Si 003 nombra las rutas de otra forma (por ejemplo, la pantalla de Ajustes), se usan las suyas. El módulo y las responsabilidades de este plan no cambian.
- `crates/core` no depende de Tauri (R7). Toda la lógica va en `crates/core/src/runtime/`. `src-tauri/` solo expone comandos y eventos.
- Decisiones del usuario: la app **descarga** la release de `ggml-org/llama.cpp` **y además** permite seleccionar un binario ya instalado.
- El gestor de descargas completo (cola, reanudación, pausa) es el **plan 006**. Aquí solo hay un descargador simple detrás del trait `ArtifactFetcher`, que es el punto de integración: 006 aportará otra implementación y `RuntimeManager` no cambiará.

### API de GitHub verificada (2026-11-26)

`GET https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10` con `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2022-11-28` y `User-Agent: app-llama` (GitHub exige `User-Agent`). Sin token, el límite es de 60 peticiones/hora por IP.

Hechos comprobados que condicionan el diseño:

1. **`/releases/latest` no sirve.** Devuelve `v1.1.0` (`prerelease: false`) con un único asset, `nightly-tag.txt`. Las builds reales son `bNNNNN` (p. ej. `b12780`) y están marcadas como **`prerelease: true`**. Por tanto, se lista `/releases` y se elige la primera release no `draft` cuyo `tag_name` cumple `^b\d+$` y que tiene un asset compatible.
2. **Cada asset trae `digest: "sha256:<hex>"`**, además de `name`, `size` y `browser_download_url`. Permite verificar la integridad (R6).
3. **Nombres de assets de `b12780`** (los relevantes para Windows y macOS):

   | Asset | Notas |
   |---|---|
   | `llama-b12780-bin-win-cpu-x64.zip` | Una sola build CPU. Las variantes AVX/AVX2/AVX512 van dentro, como backends cargables |
   | `llama-b12780-bin-win-cpu-arm64.zip` | |
   | `llama-b12780-bin-win-cuda-12.4-x64.zip`, `llama-b12780-bin-win-cuda-13.5-x64.zip`, `llama-b12780-bin-win-cuda-13.5-arm64.zip` | CUDA: dos versiones mayores |
   | `cudart-llama-bin-win-cuda-12.4-x64.zip`, `cudart-llama-bin-win-cuda-13.5-x64.zip`, `cudart-llama-bin-win-cuda-13.5-arm64.zip` | DLL `cudart`. **Sin tag en el nombre** (en Ubuntu sí lo llevan: `cudart-llama-b12780-bin-ubuntu-…`) |
   | `llama-b12780-bin-win-vulkan-x64.zip`, `llama-b12780-bin-win-vulkan-arm64.zip` | |
   | `llama-b12780-bin-macos-arm64.tar.gz` | Metal incluido. **macOS usa `.tar.gz`, no `.zip`** |
   | `llama-b12780-bin-macos-x64.tar.gz` | Intel: se trata como CPU |
   | `…-win-sycl-x64.zip`, `…-win-rocm-10.0-x64.zip`, `…-win-openvino-2026.4.1-x64.zip`, `…-win-opencl-adreno-arm64.zip`, `…-ubuntu-*`, `…-android-*`, `llama-b12780-xcframework.zip`, `llama-b12780-ui.tar.gz` | Fuera de alcance: se descartan |

4. En versiones antiguas los nombres eran otros (`…-bin-win-avx2-x64.zip`, `…-bin-win-avx512-x64.zip`, `…-bin-win-noavx-x64.zip`, `…-bin-win-cuda-cu12.4-x64.zip`, `cudart-llama-bin-win-cu12.4-x64.zip`, `…-bin-macos-arm64.zip`). Por eso **la clasificación usa tokens, no nombres fijos** (riesgo del MVP §5). Estos nombres antiguos se usan solo como fixture sintético de robustez.

### `llama-server --version`

Escribe en stderr (a veces en stdout), detrás de posibles líneas de inicialización de backends (`ggml_cuda_init: …`), algo como:

```
version: 12780 (a1b2c3d)
built with MSVC 19.44.35211.0 for Windows AMD64
```

Se busca la primera línea que cumpla `^version: (\d+) \(([0-9a-f]+)\)` en stdout y stderr. El número coincide con el del tag (`b12780` → `12780`), y así se comparan versiones.

### Diseño

```
crates/core/src/runtime/
├─ mod.rs        # re-exports + RuntimeError (thiserror)
├─ release.rs    # Release/Asset (serde) + latest_build_release
├─ asset.rs      # classify_asset: nombre → AssetDescriptor (tokens)
├─ hardware.rs   # HardwareInfo, SystemProbe (trait), parsers de nvidia-smi, detect
├─ select.rs     # select_install(release, hw, preference) → InstallPlan
├─ fetch.rs      # ArtifactFetcher (trait) + SimpleHttpFetcher (reqwest)
├─ extract.rs    # extract_archive (zip / tar.gz seguro) + find_server_binary
├─ verify.rs     # CommandRunner (trait), parse_version, verify_server, remove_quarantine
├─ state.rs      # RuntimeState persistido en <data>/runtime/runtime.json
├─ github.rs     # ReleaseSource (trait) + GitHubReleaseSource (reqwest)
└─ manager.rs    # RuntimeManager: status, check_update, install, use_external
src-tauri/src/commands/runtime.rs   # comandos + evento runtime://progress
ui/src/lib/runtime/                 # api.ts, view-model.ts (+ tests), RuntimeSettings.svelte
```

- **Tipos clave:**
  - `Os { Windows, MacOs }` y `Arch { X86_64, Aarch64 }`.
  - `Backend { Cpu { isa: CpuIsa }, Cuda { major: u32, minor: u32 }, Vulkan, Metal }`, con `CpuIsa { Generic, Avx, Avx2, Avx512 }`. `Generic` es la build CPU actual, que lleva todas las variantes dentro.
  - `BackendPreference { Auto, Cpu, Cuda, Vulkan, Metal }`.
- **Clasificación por tokens** (`classify_asset`):
  - Asset principal: `llama-[bN-]bin-<os>-<resto>.<zip|tar.gz>`. Cudart: `cudart-llama-[bN-]bin-<os>-<resto>.<ext>`.
  - `<resto>` se parte por `-`. El token de arquitectura (`x64` | `arm64`) es el último.
  - Tokens de backend:
    - `cpu` → `Cpu{Generic}`; `noavx` → `Cpu{Generic}`; `avx` / `avx2` / `avx512` → `Cpu{isa}`.
    - `cuda` seguido de `12.4` o de `cu12.4` → `Cuda{12,4}`; `cu12.4` suelto → `Cuda{12,4}`.
    - `vulkan` → `Vulkan`.
    - Sin token: `macos-arm64` → `Metal` y `macos-x64` → `Cpu{Generic}`.
  - Cualquier otro token (`sycl`, `rocm`, `hip`, `openvino`, `opencl`, `adreno`, `snapdragon`, `kleidiai`…), un SO distinto de `win`/`macos` u otro formato → `None`.
- **Selección (`select_install`)**, sobre los assets compatibles con el SO y la arquitectura de la máquina:
  - CUDA solo si hay GPU NVIDIA con `driver_cuda` detectado. Se elige la versión de mayor número cuyo `major ≤ driver_cuda.major` (compatibilidad de versión menor de CUDA). Exige el cudart de la misma versión y arquitectura; si falta, ese candidato se descarta.
  - Vulkan solo si `vulkan_available`. Metal solo en macOS arm64.
  - CPU: se prefiere `Generic`. Si no existe (releases antiguas), la mayor `isa` soportada por la CPU.
  - `Auto` = primer candidato disponible en el orden **CUDA > Metal > Vulkan > CPU**. Con una preferencia explícita no disponible → `RuntimeError::BackendUnavailable`.
  - `available_backends(release, hw)` devuelve la lista para el selector de la UI.
- **Instalación** (`RuntimeManager::install`), con un solo proceso a la vez (`AlreadyInstalling`):
  1. Listar releases → `latest_build_release`.
  2. `select_install`.
  3. Descargar el principal (y el cudart) a `<data>/runtime/.staging-<tag>/downloads/`, verificando tamaño y SHA256.
  4. Extraer a `<data>/runtime/.staging-<tag>/install/`; el cudart va al directorio de `llama-server`.
  5. En macOS, `xattr -cr <install>`.
  6. `verify_server`.
  7. Borrar `downloads/` y renombrar `install/` → `<data>/runtime/<tag>/`.
  8. Guardar el estado.
  9. Borrar el directorio de la versión gestionada anterior (si falla, por ejemplo porque Windows tiene el fichero bloqueado, se apunta en `pending_cleanup` y se reintenta en el siguiente `status`).

  Con cualquier error o cancelación se borra `.staging-<tag>` y el estado no cambia.
- **Binario externo** (`use_external(path)`): el nombre debe ser `llama-server` o `llama-server.exe` y debe pasar `verify_server`. No se modifica (sin `xattr`). Se guarda como `source: external`.
- **Progreso:** `ProgressEvent { phase: Fetching{artifact} | Extracting | Verifying | Done, done: u64, total: u64 }`. Se emite solo cuando cambia la fase o el porcentaje entero (sin reloj, determinista). Se cancela con un `CancelToken` (`Arc<AtomicBool>`) que se comprueba en cada bloque descargado y en cada entrada extraída.
- **Dependencias nuevas** (con `--locked`, en `Cargo.lock`):
  - `crates/core`: `reqwest` (rustls, stream, json), `tokio`, `serde`, `serde_json`, `sha2`, `hex`, `zip`, `flate2`, `tar`, `thiserror`, `async-trait`, `futures-util`.
  - Dev de `crates/core`: `wiremock`, `tempfile`.
  - `src-tauri`: `tauri-plugin-dialog`. `ui`: `@tauri-apps/plugin-dialog`.
  - Si 003 ya fija alguna, se reutiliza su versión.

## Criterios de aceptación

- **CA1** — Dado un `SystemProbe` falso (x86_64 con AVX2 sin AVX512; `nvidia-smi` con `RTX 4070, 12282 MiB, CUDA Version: 13.0`; `vulkan-1.dll` presente), cuando se llama a `detect`, entonces `HardwareInfo` = `{ os: Windows, arch: X86_64, avx2: true, avx512: false, gpus: [{ name: "NVIDIA GeForce RTX 4070", vram_mib: 12282 }], driver_cuda: Some(13.0), vulkan_available: true, metal: false }`. Si `nvidia-smi` no existe o falla, `gpus` queda vacío y `driver_cuda: None`, sin error.
- **CA2** — Dado el fixture de `b12780`, cuando se llama a `select_install` con `Auto`, entonces el asset elegido es:
  - Windows x64 con driver CUDA 13.0 → `llama-b12780-bin-win-cuda-13.5-x64.zip` + `cudart-llama-bin-win-cuda-13.5-x64.zip`.
  - Driver CUDA 12.8 → `…-cuda-12.4-x64.zip` + `cudart-…-cuda-12.4-x64.zip`.
  - Driver CUDA 11.8 con Vulkan → `…-win-vulkan-x64.zip`.
  - Sin GPU ni Vulkan → `…-win-cpu-x64.zip`.
  - macOS arm64 → `…-macos-arm64.tar.gz` (Metal).
  - macOS x64 → `…-macos-x64.tar.gz`.
- **CA3** — Dado el fixture sintético de nombres antiguos, cuando se selecciona para CPU con AVX2 sin AVX512, entonces se elige `…-win-avx2-x64.zip`. Con CUDA 12, `…-win-cuda-cu12.4-x64.zip` + `cudart-llama-bin-win-cu12.4-x64.zip`.
- **CA4** — Dada una lista de releases con `v1.1.0` primero, después un `draft` y después `b12780`, cuando se llama a `latest_build_release`, entonces devuelve `b12780`. Si ninguna release tiene un asset compatible, el error es `NoCompatibleRelease`.
- **CA5** — Dado un servidor HTTP local que sirve un artefacto, cuando el `digest` o el tamaño no coinciden, entonces `fetch` devuelve `IntegrityMismatch` o `SizeMismatch`, no queda ningún fichero en el destino y el estado del runtime no cambia.
- **CA6** — Dado un zip o tar.gz con una entrada `../evil` o una ruta absoluta, cuando se extrae, entonces se devuelve `UnsafeArchivePath` y no se escribe nada fuera del directorio de destino. En Unix, un tar.gz con `llama-server` en modo `0755` conserva el bit de ejecución.
- **CA7** — Dado un `llama-server` cuyo `--version` falla o no muestra `version: N (hash)`, cuando se instala, entonces `install` devuelve `VerificationFailed`, `.staging-<tag>` se borra y el runtime activo anterior sigue intacto (mismo `runtime.json`).
- **CA8** — Dada una instalación gestionada en macOS, cuando se instala, entonces se ejecuta `xattr -cr <directorio de instalación>` antes de `--version`. En Windows no se ejecuta. Para un binario externo no se ejecuta nunca.
- **CA9** — Dada la ruta de un `llama-server[.exe]` válido, cuando se llama a `use_external`, entonces queda activo con `source: external` y su versión. Si el nombre no es `llama-server[.exe]` o la verificación falla, el error es `InvalidExternalBinary` o `VerificationFailed` y el estado no cambia.
- **CA10** — Dado `b12779` gestionado y activo con `b12780` publicada, cuando se llama a `check_update`, entonces `{ installed: 12779, latest: 12780, update_available: true }`. Tras `install`, queda activo `b12780` y el directorio `runtime/b12779/` ya no existe. Con `b12780` instalado, `update_available: false`.
- **CA11** — Dado `runtime.json` escrito por una instalación, cuando se crea un `RuntimeManager` nuevo sobre el mismo directorio, entonces `status` devuelve el mismo runtime activo. Si el binario ya no existe, devuelve `Broken { path }`. Si `runtime.json` está corrupto, devuelve `NotInstalled` y conserva una copia `runtime.json.bak`.
- **CA12** — Dada la pantalla Ajustes → llama.cpp, cuando se abre, entonces muestra:
  - el hardware detectado;
  - el runtime activo (versión `bN`, backend, origen gestionado o externo, y ruta) o "No instalado";
  - un selector de backend con solo los disponibles;
  - los botones "Instalar" o "Actualizar a bN", "Buscar actualizaciones" y "Usar binario existente…".

  Durante la instalación muestra la fase y el porcentaje (evento `runtime://progress`) y un botón "Cancelar". Lo cubren los tests de `view-model.ts`.
- **CA13** — Dado un PC Windows x64 real y un Mac Apple Silicon real, cuando se pulsa "Instalar" en Ajustes, entonces termina con la versión `bN` visible, y `llama-server --version`, ejecutado a mano desde el directorio instalado, da el mismo número. Comprobación manual en T13.
- **CA14** — Dado el plan aplicado, cuando se ejecuta `node scripts/quality-gate.mjs`, entonces termina en verde en Windows y macOS (CI).

## Tareas

> Todas las tareas de código son **(developer)** con TDD: primero los tests de la tabla (rojo) y después el código (verde). **(O)** = orchestrator. Tests sin red real (servidor local con `wiremock`), sin reloj y sin aleatoriedad (R3). Ficheros temporales con `tempfile`. Rutas con `Path`/`PathBuf`. Fixtures en `crates/core/tests/fixtures/runtime/`.

### T1 — Modelo de releases y elección de la última build
- **Ficheros/módulos:** `crates/core/src/runtime/mod.rs` (`RuntimeError` inicial), `crates/core/src/runtime/release.rs`, fixtures `releases_b12780.json` (respuesta real recortada a `v1.1.0` + `b12780` + `b12779`, con solo los assets de la tabla del Contexto) y `releases_legacy.json` (sintético con nombres antiguos)
- **Depende de:** — (plan 003)
- **Cubre:** CA4
- **Subtareas:**
  - [ ] T1.1 `Release { tag_name, draft, prerelease, assets }` y `Asset { name, size, digest: Option<String>, browser_download_url }` con `serde` (los campos desconocidos se ignoran).
  - [ ] T1.2 `parse_releases(json: &str) -> Result<Vec<Release>, RuntimeError>`, `build_number(tag) -> Option<u32>` y `Asset::sha256() -> Option<[u8; 32]>` (solo si es `sha256:` + 64 hex).
  - [ ] T1.3 `latest_build_release(releases, is_compatible: impl Fn(&Release) -> bool) -> Result<&Release, RuntimeError>`: la de mayor `build_number` entre las no `draft` con tag `^b\d+$` que cumplen el predicado. `prerelease` no se tiene en cuenta.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `parse_releases_fixture_real` | `releases_b12780.json` | 3 releases; `b12780` con `prerelease: true` y el asset `llama-b12780-bin-win-cpu-x64.zip` con `size` y `digest` |
  | `parse_releases_json_invalido` | `"{no json"` | `Err(RuntimeError::ReleaseParse(_))` |
  | `build_number_tags` | `b12780`, `b1`, `v1.1.0`, `b12a`, `B12`, `""` | `Some(12780)`, `Some(1)`, `None`, `None`, `None`, `None` |
  | `sha256_digest_valido` | `"sha256:" + 64 hex` | `Some(bytes)` |
  | `sha256_digest_invalido_o_ausente` | `None`, `"md5:…"`, `"sha256:abc"`, `"sha256:" + 64 'z'` | `None` |
  | `ultima_build_ignora_v_y_draft` | `[v1.1.0, b12781 draft, b12780, b12779]` con el predicado siempre cierto | `b12780` |
  | `ultima_build_respeta_predicado` | `b12780` sin assets compatibles y `b12779` con ellos | `b12779` |
  | `ultima_build_no_depende_del_orden` | `[b12779, b12780]` | `b12780` |
  | `ultima_build_sin_compatibles` | solo `v1.1.0` | `Err(NoCompatibleRelease)` |

### T2 — Clasificación de assets por tokens
- **Ficheros/módulos:** `crates/core/src/runtime/asset.rs`
- **Depende de:** T1
- **Cubre:** CA2, CA3 (base)
- **Subtareas:**
  - [ ] T2.1 Tipos `Os`, `Arch`, `Backend`, `CpuIsa`, `ArchiveFormat { Zip, TarGz }`, `AssetKind { Main, Cudart }` y `AssetDescriptor { kind, os, arch, backend, format }`.
  - [ ] T2.2 `classify_asset(name: &str) -> Option<AssetDescriptor>`, según el Diseño. Para `Cudart`, `backend` es `Cuda{..}`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `clasifica_assets_actuales` | los 12 nombres Windows/macOS de la tabla del Contexto | descriptor exacto por nombre (tabla en el test) |
  | `clasifica_cudart_sin_tag` | `cudart-llama-bin-win-cuda-13.5-x64.zip` | `Cudart`, `Windows`, `X86_64`, `Cuda{13,5}`, `Zip` |
  | `clasifica_cudart_con_tag` | `cudart-llama-b12780-bin-win-cuda-12.4-x64.zip` | `Cudart`, `Cuda{12,4}` |
  | `clasifica_nombres_antiguos` | `llama-b4500-bin-win-avx2-x64.zip`, `…-avx512-x64.zip`, `…-noavx-x64.zip`, `…-cuda-cu12.4-x64.zip`, `cudart-llama-bin-win-cu12.4-x64.zip`, `llama-b4500-bin-macos-arm64.zip` | `Cpu{Avx2}`, `Cpu{Avx512}`, `Cpu{Generic}`, `Cuda{12,4}`, `Cudart Cuda{12,4}`, `Metal Zip` |
  | `descarta_backends_fuera_de_alcance` | `…-win-sycl-x64.zip`, `…-win-rocm-10.0-x64.zip`, `…-win-openvino-2026.4.1-x64.zip`, `…-win-opencl-adreno-arm64.zip`, `…-win-hip-radeon-x64.zip` | `None` |
  | `descarta_otros_so_y_formatos` | `…-ubuntu-x64.tar.gz`, `…-android-arm64.tar.gz`, `llama-b12780-xcframework.zip`, `llama-b12780-ui.tar.gz`, `nightly-tag.txt`, `llama-b1-bin-win-cpu-x64.7z` | `None` |
  | `descarta_nombres_malformados` | `""`, `llama-`, `llama-bin-win.zip`, `llama-b1-bin-win-cuda-x64.zip`, `llama-b1-bin-win-cuda-12-x64.zip` | `None` |
  | `clasificacion_sensible_a_mayusculas` | `LLAMA-B1-BIN-WIN-CPU-X64.ZIP` | `None` |

### T3 — Detección de hardware
- **Ficheros/módulos:** `crates/core/src/runtime/hardware.rs`
- **Depende de:** T2 (`Os`, `Arch`)
- **Cubre:** CA1
- **Subtareas:**
  - [ ] T3.1 `GpuInfo { name, vram_mib: Option<u64> }`, `CudaVersion { major, minor }` y `HardwareInfo { os, arch, avx2, avx512, gpus, driver_cuda: Option<CudaVersion>, vulkan_available, metal }` (`Serialize`).
  - [ ] T3.2 Parsers puros:
    - `parse_nvidia_smi_gpus(csv: &str) -> Vec<GpuInfo>` (salida de `nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits`).
    - `parse_nvidia_smi_cuda(header: &str) -> Option<CudaVersion>` (busca `CUDA Version: X.Y` en la salida de `nvidia-smi` sin argumentos).
  - [ ] T3.3 Trait `SystemProbe`:
    - `os()`, `arch()`, `cpu_has(feature: &str) -> bool`;
    - `run(program, args) -> Option<String>` (stdout si el código de salida es 0);
    - `file_exists(&Path)`, `system_root() -> Option<PathBuf>`.
  - [ ] T3.4 `detect(probe: &dyn SystemProbe) -> HardwareInfo`:
    - `avx2`/`avx512` (`avx512f`) solo en x86_64.
    - `nvidia-smi` solo en Windows.
    - Vulkan: en Windows, existe `<system_root>/System32/vulkan-1.dll`. En macOS siempre `vulkan_available = false` (no hay build Vulkan para macOS).
    - `metal = os == MacOs && arch == Aarch64`.
  - [ ] T3.5 `NativeProbe`: implementación real con `std::env::consts`, `is_x86_feature_detected!` (bajo `cfg(target_arch = "x86_64")`), `std::process::Command` y `%SystemRoot%`.
- **Tests unitarios** (con `FakeProbe` configurable):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `parse_gpus_una` | `"NVIDIA GeForce RTX 4070, 12282\n"` | `[{ name: "NVIDIA GeForce RTX 4070", vram_mib: Some(12282) }]` |
  | `parse_gpus_varias_y_espacios` | dos líneas con espacios y `\r\n` | 2 GPU recortadas |
  | `parse_gpus_vram_no_numerica` | `"Tesla X, [N/A]"` | `vram_mib: None` |
  | `parse_gpus_vacio` | `""` | `[]` |
  | `parse_cuda_cabecera_real` | cabecera de `nvidia-smi` con `CUDA Version: 13.0` (fixture `nvidia_smi_header.txt`) | `Some(13.0)` |
  | `parse_cuda_ausente_o_raro` | `""`, `CUDA Version: N/A`, `CUDA Version: 12` | `None` |
  | `detect_windows_nvidia_vulkan` | probe de CA1 | `HardwareInfo` de CA1 |
  | `detect_sin_nvidia_smi` | `run` → `None` | `gpus: []`, `driver_cuda: None` |
  | `detect_sin_vulkan` | `vulkan-1.dll` ausente | `vulkan_available: false` |
  | `detect_sin_system_root` | `system_root` → `None` | `vulkan_available: false`, sin pánico |
  | `detect_macos_arm64` | `MacOs`, `Aarch64` | `metal: true`, `avx2: false`, `vulkan_available: false`, sin llamar a `run` |
  | `detect_macos_x64` | `MacOs`, `X86_64`, AVX2 | `metal: false`, `avx2: true` |
  | `native_probe_no_falla` | `detect(&NativeProbe)` | devuelve `os`/`arch` iguales a `std::env::consts` (sin comprobar el hardware) |

### T4 — Selección de asset y cudart
- **Ficheros/módulos:** `crates/core/src/runtime/select.rs`
- **Depende de:** T1, T2, T3
- **Cubre:** CA2, CA3
- **Subtareas:**
  - [ ] T4.1 `BackendPreference` (`Deserialize`) e `InstallPlan { tag, backend, main: Asset, cudart: Option<Asset> }`.
  - [ ] T4.2 `available_backends(release, hw) -> Vec<Backend>`, en el orden CUDA > Metal > Vulkan > CPU, con las reglas del Diseño.
  - [ ] T4.3 `select_install(release, hw, preference) -> Result<InstallPlan, RuntimeError>` (`NoCompatibleAsset` o `BackendUnavailable`) e `is_compatible(release, hw) -> bool` (predicado para T1.3).
- **Tests unitarios** (fixtures de T1):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `auto_cuda_13` | b12780, Win x64, `driver_cuda 13.0` | main `…-win-cuda-13.5-x64.zip`, cudart `cudart-llama-bin-win-cuda-13.5-x64.zip` |
  | `auto_cuda_12` | driver 12.8 | `…-cuda-12.4-x64.zip` + `cudart-…-12.4-x64.zip` |
  | `auto_cuda_driver_antiguo_vulkan` | driver 11.8, Vulkan | `…-win-vulkan-x64.zip`, sin cudart |
  | `auto_cpu` | sin GPU ni Vulkan | `…-win-cpu-x64.zip` |
  | `auto_win_arm64` | Win arm64, sin GPU | `…-win-cpu-arm64.zip` |
  | `auto_macos_arm64_metal` | macOS arm64 | `…-macos-arm64.tar.gz`, `Metal` |
  | `auto_macos_x64` | macOS x64 | `…-macos-x64.tar.gz`, `Cpu{Generic}` |
  | `cuda_sin_cudart_se_descarta` | b12780 sin los `cudart-*`, driver 13.0, Vulkan | Vulkan |
  | `preferencia_explicita_cpu` | driver 13.0 + `Cpu` | `…-win-cpu-x64.zip` |
  | `preferencia_no_disponible` | sin GPU + `Cuda` | `Err(BackendUnavailable)` |
  | `legacy_cpu_mejor_isa` | legacy, AVX2 sin AVX512 | `…-win-avx2-x64.zip` |
  | `legacy_cpu_avx512` | legacy, AVX512 | `…-win-avx512-x64.zip` |
  | `legacy_cpu_sin_avx` | legacy, sin AVX | `…-win-noavx-x64.zip` |
  | `legacy_cuda_cu` | legacy, driver 12.2 | `…-cuda-cu12.4-x64.zip` + `cudart-llama-bin-win-cu12.4-x64.zip` |
  | `backends_disponibles_orden` | b12780, Win x64, driver 13.0, Vulkan | `[Cuda{13,5}, Vulkan, Cpu{Generic}]` |
  | `release_sin_compatibles` | release con solo `ubuntu-*` | `Err(NoCompatibleAsset)`; `is_compatible` → `false` |

### T5 — Descargador simple (punto de integración con 006)
- **Ficheros/módulos:** `crates/core/src/runtime/fetch.rs`
- **Depende de:** T1
- **Cubre:** CA5
- **Subtareas:**
  - [ ] T5.1 `#[async_trait] trait ArtifactFetcher { async fn fetch(&self, req: FetchRequest, progress: &(dyn Fn(u64, u64) + Send + Sync), cancel: &CancelToken) -> Result<(), RuntimeError>; }`, con `FetchRequest { url, dest: PathBuf, expected_size: u64, sha256: Option<[u8; 32]> }`. Doc-comment: "el plan 006 aportará una implementación con reanudación".
  - [ ] T5.2 `SimpleHttpFetcher` (con `reqwest::Client` inyectado):
    - Descarga en streaming a `<dest>.part`, calculando el SHA256 sobre la marcha y llamando a `progress(done, total)` por bloque.
    - Al terminar, comprueba tamaño y hash y renombra a `dest`.
    - Con error HTTP, de tamaño, de hash o cancelación, borra `.part`.
  - [ ] T5.3 `CancelToken` (`Arc<AtomicBool>`, `cancel()`, `is_cancelled()`).
- **Tests unitarios** (`wiremock` en `127.0.0.1`, `tempfile`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `fetch_ok_verifica_hash_y_tamano` | 1 MiB fijo (bytes `i % 251`) + su SHA256 | `Ok`, `dest` existe con esos bytes y no queda `.part` |
  | `fetch_sin_digest_solo_tamano` | `sha256: None` | `Ok` |
  | `fetch_hash_distinto` | hash de otro contenido | `Err(IntegrityMismatch)`, ni `dest` ni `.part` |
  | `fetch_tamano_distinto` | `expected_size` + 1 | `Err(SizeMismatch{ expected, actual })`, sin ficheros |
  | `fetch_http_404` | 404 | `Err(HttpStatus(404))`, sin ficheros |
  | `fetch_cancelado` | token cancelado antes del primer bloque | `Err(Cancelled)`, sin ficheros |
  | `fetch_progreso_monotono` | 1 MiB | llamadas con `done` creciente; la última es `(1048576, 1048576)` |
  | `fetch_sigue_redirecciones` | 302 → otra ruta del mock (como `browser_download_url` → CDN) | `Ok` |

### T6 — Extracción segura y localización de `llama-server`
- **Ficheros/módulos:** `crates/core/src/runtime/extract.rs`
- **Depende de:** T2 (`ArchiveFormat`), T5 (`CancelToken`)
- **Cubre:** CA6
- **Subtareas:**
  - [ ] T6.1 `extract_archive(archive: &Path, format, dest: &Path, cancel) -> Result<(), RuntimeError>`:
    - Zip con `enclosed_name()`; tar.gz con `Entry::unpack_in` y modos preservados.
    - Rechaza las rutas absolutas, `..` y los enlaces simbólicos que apuntan fuera de `dest` (`UnsafeArchivePath`).
  - [ ] T6.2 `find_server_binary(root: &Path, os) -> Result<PathBuf, RuntimeError>`: busca `llama-server.exe` (Windows) o `llama-server` (macOS) con profundidad ≤ 4, sin suponer la estructura del archivo. Si hay varios, el menos profundo; con empate, el primero en orden lexicográfico. Si no hay ninguno, `ServerBinaryNotFound`.
  - [ ] T6.3 Los archivos de prueba se generan en el propio test con `zip`/`tar` en un `tempdir` (sin binarios versionados).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `zip_plano_se_extrae` | zip con `llama-server.exe`, `ggml.dll` | los 2 ficheros en `dest` con su contenido |
  | `targz_con_carpeta_se_extrae` | `llama-b1/llama-server`, `llama-b1/libllama.dylib` | estructura conservada |
  | `targz_preserva_ejecutable` (`cfg(unix)`) | `llama-server` con `0o755` | el modo extraído tiene `0o111` |
  | `zip_slip_rechazado` | entrada `../evil.txt` | `Err(UnsafeArchivePath)`, no existe `<dest>/../evil.txt` |
  | `targz_ruta_absoluta_rechazada` | entrada `/tmp/evil` | `Err(UnsafeArchivePath)` |
  | `targz_symlink_fuera_rechazado` (`cfg(unix)`) | enlace `lib -> ../../etc` | `Err(UnsafeArchivePath)` |
  | `archivo_corrupto` | bytes aleatorios fijos `b"not a zip"` | `Err(Extract(_))` |
  | `extraccion_cancelada` | token cancelado | `Err(Cancelled)` |
  | `busca_server_en_raiz` | `dest/llama-server.exe` (Windows) | esa ruta |
  | `busca_server_anidado` | `dest/build/bin/llama-server` (macOS) | esa ruta |
  | `busca_server_el_menos_profundo` | `a/llama-server` y `a/b/llama-server` | `a/llama-server` |
  | `busca_server_ausente` | solo `llama-cli` | `Err(ServerBinaryNotFound)` |
  | `busca_server_ignora_nombre_parecido` | `llama-server-old`, `llama-server.exe.bak` | `Err(ServerBinaryNotFound)` |

### T7 — Verificación con `--version` y cuarentena de macOS
- **Ficheros/módulos:** `crates/core/src/runtime/verify.rs`, fixtures `version_cpu.txt`, `version_cuda_stderr.txt`
- **Depende de:** T2
- **Cubre:** CA7, CA8
- **Subtareas:**
  - [ ] T7.1 `RuntimeVersion { build: u32, commit: String }` y `parse_version(stdout, stderr) -> Option<RuntimeVersion>`.
  - [ ] T7.2 `#[async_trait] trait CommandRunner { async fn run(&self, program: &Path, args: &[&str], cwd: Option<&Path>) -> Result<CommandOutput, RuntimeError>; }` y `TokioCommandRunner` (`tokio::process`, `kill_on_drop`, timeout de 15 s y, en Windows, `CREATE_NO_WINDOW`).
  - [ ] T7.3 `verify_server(runner, server: &Path) -> Result<RuntimeVersion, RuntimeError>`: ejecuta `--version` con `cwd` = el directorio del binario (los DLL se resuelven allí). Si el código de salida es distinto de 0 o no se puede parsear, `VerificationFailed { stderr_tail }` (últimos 2000 caracteres).
  - [ ] T7.4 `remove_quarantine(runner, os, dir)`: en macOS ejecuta `/usr/bin/xattr -cr <dir>`; en Windows no hace nada. Si `xattr` falla, `QuarantineFailed`.
- **Tests unitarios** (con `FakeRunner`, que registra las llamadas):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `version_en_stderr` | `version_cpu.txt` en stderr | `{ build: 12780, commit: "a1b2c3d" }` |
  | `version_tras_lineas_cuda` | `version_cuda_stderr.txt` (líneas `ggml_cuda_init: …` antes) | `12780` |
  | `version_en_stdout` | la línea en stdout y stderr vacío | `12780` |
  | `version_con_crlf` | `version: 5 (abc)\r\n` | `{ 5, "abc" }` |
  | `version_no_parseable` | `""`, `version: x (abc)`, `version 12 (abc)`, `version: 12 abc` | `None` |
  | `verify_ok_usa_cwd_del_binario` | fake con salida 0 + versión | `Ok`; la llamada es `[server, ["--version"], Some(dir)]` |
  | `verify_codigo_no_cero` | salida 1, stderr `"missing cudart64_12.dll"` | `Err(VerificationFailed)` con ese texto |
  | `verify_sin_version` | salida 0, sin línea de versión | `Err(VerificationFailed)` |
  | `verify_error_al_lanzar` | el runner devuelve `Err(Io)` | `Err(VerificationFailed)` |
  | `quarantine_macos_llama_xattr` | `MacOs`, `dir` | una llamada `/usr/bin/xattr ["-cr", dir]` |
  | `quarantine_windows_no_hace_nada` | `Windows` | 0 llamadas |
  | `quarantine_fallo` | `xattr` con salida 1 | `Err(QuarantineFailed)` |
  | `runner_real_programa_inexistente` | `TokioCommandRunner` con una ruta inexistente en `tempdir` | `Err(Io)` |

### T8 — Estado persistido del runtime
- **Ficheros/módulos:** `crates/core/src/runtime/state.rs`
- **Depende de:** T2, T7
- **Cubre:** CA11
- **Subtareas:**
  - [ ] T8.1 `RuntimeState { schema: 1, active: Option<ActiveRuntime>, pending_cleanup: Vec<PathBuf> }` y `ActiveRuntime { source: Managed { tag } | External, backend, server_path, version: RuntimeVersion }` (`serde`, JSON legible).
  - [ ] T8.2 `StateStore::new(runtime_dir)`:
    - `load()`: si no existe → `Default`; si está corrupto → copia a `runtime.json.bak` y devuelve `Default`.
    - `save()`: atómico (`runtime.json.tmp` + `rename`).
  - [ ] T8.3 `RuntimeStatus { NotInstalled, Ready(ActiveRuntime), Broken { path } }` y `status_of(state, exists: impl Fn(&Path) -> bool)`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `estado_roundtrip_gestionado` | `Managed { tag: "b12780" }`, `Cuda{13,5}` | `save` + `load` → igual |
  | `estado_roundtrip_externo` | `External` con ruta con espacios y no ASCII (`Él dir`) | igual |
  | `estado_inexistente` | directorio vacío | `Default` (`active: None`) |
  | `estado_corrupto_hace_copia` | `runtime.json` = `"{"` | `Default` y existe `runtime.json.bak` con `"{"` |
  | `estado_schema_desconocido` | `schema: 99` | `Default` + `.bak` |
  | `estado_guardado_atomico` | `save` | no queda `runtime.json.tmp` |
  | `status_no_instalado` | `active: None` | `NotInstalled` |
  | `status_listo` | activo + `exists` → `true` | `Ready(..)` |
  | `status_roto` | activo + `exists` → `false` | `Broken { path }` |

### T9 — Cliente de releases de GitHub
- **Ficheros/módulos:** `crates/core/src/runtime/github.rs`
- **Depende de:** T1
- **Cubre:** CA4 (red), base de CA10
- **Subtareas:**
  - [ ] T9.1 `#[async_trait] trait ReleaseSource { async fn list_releases(&self) -> Result<Vec<Release>, RuntimeError>; }`.
  - [ ] T9.2 `GitHubReleaseSource { client, base_url }`: `GET {base_url}/repos/ggml-org/llama.cpp/releases?per_page=10` con las cabeceras del Contexto. `base_url` se inyecta (en producción, `https://api.github.com`).
    - 403 o 429 con `x-ratelimit-remaining: 0` → `RateLimited { reset_epoch: Option<u64> }` (de `x-ratelimit-reset`).
    - Otro estado distinto de 2xx → `HttpStatus(code)`.
- **Tests unitarios** (`wiremock`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `lista_releases_ok` | 200 + `releases_b12780.json` | 3 releases |
  | `lista_releases_envia_cabeceras` | el mock exige `User-Agent: app-llama`, `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2022-11-28` y `per_page=10` | `Ok` (si falta alguna, el mock responde 404 y el test falla) |
  | `lista_releases_rate_limit` | 403, `x-ratelimit-remaining: 0`, `x-ratelimit-reset: 1764150000` | `Err(RateLimited { reset_epoch: Some(1764150000) })` |
  | `lista_releases_403_sin_rate_limit` | 403 sin cabeceras | `Err(HttpStatus(403))` |
  | `lista_releases_500` | 500 | `Err(HttpStatus(500))` |
  | `lista_releases_json_roto` | 200 + `"<html>"` | `Err(ReleaseParse(_))` |

### T10 — `RuntimeManager`: estado, actualización, instalación y binario externo
- **Ficheros/módulos:** `crates/core/src/runtime/manager.rs`
- **Depende de:** T3–T9
- **Cubre:** CA5, CA7, CA8, CA9, CA10, CA11
- **Subtareas:**
  - [ ] T10.1 `RuntimeManager::new(runtime_dir, deps: RuntimeDeps { source, fetcher, runner, probe })`, con dependencias `Arc<dyn …>` inyectadas. Un `Mutex` impide dos instalaciones a la vez (`AlreadyInstalling`).
  - [ ] T10.2 `status()`: `status_of` + reintenta borrar `pending_cleanup`. `hardware()` → `detect`. `active_server_path() -> Option<PathBuf>`: punto de integración para el plan del servidor.
  - [ ] T10.3 `check_update() -> UpdateInfo { installed: Option<u32>, latest: u32, latest_tag, available_backends, update_available }`.
  - [ ] T10.4 `install(preference, on_progress, cancel) -> Result<ActiveRuntime, RuntimeError>`: los 9 pasos del Diseño, con limpieza de `.staging-<tag>` ante cualquier error.
  - [ ] T10.5 `use_external(path) -> Result<ActiveRuntime, RuntimeError>`. No borra la versión gestionada previa (se mantiene en disco hasta la siguiente instalación gestionada; ver Preguntas abiertas).
- **Tests unitarios** (con `FakeSource`, `FakeFetcher` que escribe un zip o tar.gz generado en el test, `FakeRunner` y `FakeProbe`; `tempdir` como `runtime_dir`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `install_cpu_windows_ok` | Win x64 sin GPU, `Auto` | `runtime/b12780/llama-server.exe` existe, `runtime.json` es `Managed{b12780}` `Cpu{Generic}`, no queda `.staging-*` |
  | `install_cuda_extrae_cudart_junto_al_server` | driver 13.0; el zip principal tiene `bin/llama-server.exe`; el cudart, `cudart64_13.dll` | `runtime/b12780/bin/cudart64_13.dll` existe; 2 `fetch` |
  | `install_macos_quita_cuarentena_antes_de_verificar` | macOS arm64 | orden de llamadas al runner: `xattr -cr <staging/install>` y después `--version` |
  | `install_verificacion_falla_conserva_anterior` | activo `b12779`; `--version` con salida 1 | `Err(VerificationFailed)`, `runtime.json` sin cambios, `runtime/b12779/` intacto, sin `.staging-*` |
  | `install_hash_falla_no_cambia_estado` | el fetcher devuelve `IntegrityMismatch` | error propagado, sin `.staging-*` |
  | `install_cancelado` | token cancelado durante el fetch | `Err(Cancelled)`, sin `.staging-*` |
  | `install_concurrente_rechazado` | segunda llamada mientras la primera espera en el fetcher (sincronizado con `tokio::sync::Notify`) | `Err(AlreadyInstalling)` |
  | `install_progreso_por_fases` | instalación OK | eventos en orden `Fetching(main)` → `Extracting` → `Verifying` → `Done`; sin dos eventos seguidos con la misma fase y el mismo porcentaje |
  | `update_reemplaza_y_borra_anterior` | activo `b12779` gestionado y source con `b12780` | activo `b12780`, `runtime/b12779/` no existe |
  | `update_borrado_fallido_queda_pendiente` | el directorio anterior sustituido por un fichero no borrable simulado (`remove_dir_all` inyectado que falla) | `pending_cleanup` contiene la ruta; el siguiente `status()` la reintenta |
  | `check_update_disponible` | activo `12779`, latest `b12780` | `update_available: true`, `latest: 12780` |
  | `check_update_al_dia` | activo `12780` | `update_available: false` |
  | `check_update_sin_instalar` | `active: None` | `installed: None`, `update_available: true` |
  | `check_update_rate_limit` | source → `RateLimited` | error propagado sin tocar el estado |
  | `external_valido` | `tempdir/llama-server.exe` + versión 12000 | `Ready(External, 12000)`; el runner no recibe `xattr` |
  | `external_nombre_invalido` | `tempdir/llama-cli.exe` | `Err(InvalidExternalBinary)`, estado sin cambios |
  | `external_verificacion_falla` | `--version` con salida 1 | `Err(VerificationFailed)`, estado sin cambios |
  | `status_persistente_entre_instancias` | `install` OK y después un `RuntimeManager` nuevo | el mismo `Ready(..)` |

### T11 — Comandos y eventos Tauri
- **Ficheros/módulos:** `src-tauri/src/commands/runtime.rs` (nuevo), `src-tauri/src/lib.rs` (registro), `src-tauri/Cargo.toml` (`tauri-plugin-dialog`), `src-tauri/capabilities/*.json` (`dialog:allow-open`)
- **Depende de:** T10
- **Cubre:** CA12 (backend)
- **Subtareas:**
  - [ ] T11.1 Estado gestionado `RuntimeAppState { manager: Arc<RuntimeManager>, cancel: Mutex<Option<CancelToken>> }`, creado en `setup` con `app.path().app_data_dir()/runtime` y las implementaciones reales.
  - [ ] T11.2 Comandos: `runtime_hardware`, `runtime_status`, `runtime_check_update`, `runtime_install(preference)`, `runtime_cancel_install` y `runtime_use_external(path)`. Los errores se serializan como `{ code, message }` (`code` = nombre de la variante de `RuntimeError`, `message` en español).
  - [ ] T11.3 `runtime_install` emite `runtime://progress` con `ProgressEvent` serializado (`camelCase`).
  - [ ] T11.4 Función pura `error_to_payload(&RuntimeError) -> ErrorPayload`, testeable sin Tauri.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `error_payload_codigos` | `RateLimited{..}`, `VerificationFailed{..}`, `Cancelled`, `NoCompatibleAsset` | `code` = `"RateLimited"`, `"VerificationFailed"`, `"Cancelled"`, `"NoCompatibleAsset"`; `message` no vacío |
  | `error_payload_rate_limit_mensaje` | `RateLimited { reset_epoch: Some(..) }` | `message` contiene "límite de GitHub" |
  | `progress_event_serializa_camel_case` | `ProgressEvent { phase: Fetching{ artifact: "main" }, done: 1, total: 2 }` | JSON `{"phase":{"kind":"fetching","artifact":"main"},"done":1,"total":2}` |
  | `preference_deserializa` | `"auto"`, `"cuda"`, `"x"` | `Auto`, `Cuda`, error |

### T12 — Pantalla Ajustes → llama.cpp
- **Ficheros/módulos:** `ui/src/lib/runtime/api.ts` (envoltura de `invoke`/`listen`), `ui/src/lib/runtime/view-model.ts` + `view-model.test.ts`, `ui/src/lib/runtime/RuntimeSettings.svelte`, la pantalla de Ajustes de 003 (incluye la sección), `ui/package.json` (`@tauri-apps/plugin-dialog`)
- **Depende de:** T11
- **Cubre:** CA12
- **Subtareas:**
  - [ ] T12.1 Tipos TS espejo de `HardwareInfo`, `RuntimeStatus`, `UpdateInfo`, `ProgressEvent` y `ErrorPayload`.
  - [ ] T12.2 `view-model.ts` (funciones puras):
    - `describeHardware(hw)`;
    - `describeStatus(status)`;
    - `primaryAction(status, update)` → `{ label: "Instalar" | "Actualizar a bN" | "Al día", enabled }`;
    - `backendOptions(update)`;
    - `progressLabel(event)` → `"Descargando llama.cpp… 42 %"`, etc.;
    - `errorMessage(payload)`.
  - [ ] T12.3 `RuntimeSettings.svelte` (Svelte 5, runes): sin lógica propia; usa `view-model` y `api`. "Usar binario existente…" abre el diálogo de `plugin-dialog` filtrado por `llama-server` / `llama-server.exe`. "Cancelar" solo se ve durante la instalación.
- **Tests unitarios** (Vitest, sin Tauri):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `describe_hardware_nvidia` | `HardwareInfo` de CA1 | contiene `"AVX2"`, `"NVIDIA GeForce RTX 4070 (12 GB)"`, `"Vulkan"`; no contiene `"AVX512"` |
  | `describe_hardware_mac` | macOS arm64 | contiene `"Apple Silicon (Metal)"` |
  | `describe_status_no_instalado` | `NotInstalled` | `"No instalado"` |
  | `describe_status_listo_gestionado` | `Ready(Managed b12780, Cuda 13.5)` | `"b12780 · CUDA 13.5 · gestionado"` |
  | `describe_status_externo` | `Ready(External, 12000)` | contiene `"externo"` y la ruta |
  | `describe_status_roto` | `Broken { path }` | contiene `"no se encuentra"` y la ruta |
  | `accion_instalar` | `NotInstalled` | `{ label: "Instalar", enabled: true }` |
  | `accion_actualizar` | `installed: 12779`, `latest: 12780` | `"Actualizar a b12780"` |
  | `accion_al_dia` | `update_available: false` | `{ label: "Al día", enabled: false }` |
  | `accion_sin_info_update` | `update` = `null` (sin consultar todavía) y `Ready` | `{ label: "Buscar actualizaciones", enabled: true }` |
  | `opciones_backend` | `available_backends: [Cuda 13.5, Vulkan, Cpu]` | `["auto", "cuda", "vulkan", "cpu"]` con etiquetas en español |
  | `progreso_etiquetas` | `Fetching(main) 42/100`, `Fetching(cudart)`, `Extracting`, `Verifying`, `total: 0` | `"Descargando llama.cpp… 42 %"`, `"Descargando DLL de CUDA… N %"`, `"Extrayendo…"`, `"Verificando…"`, sin división por cero (`"Descargando llama.cpp…"`) |
  | `mensaje_error_rate_limit` | `{ code: "RateLimited", message }` | incluye "Inténtalo más tarde" |

### T13 — Verificación manual en Windows y macOS (O)
- **Depende de:** T1–T12 integradas
- **Cubre:** CA13
- **Subtareas:**
  - [ ] T13.1 Windows x64 (con GPU NVIDIA si la hay): `npm --prefix ui run tauri dev` (o el comando de 003) → Ajustes → Instalar (`Auto`) → versión visible. Ejecutar a mano `runtime\bNNNNN\…\llama-server.exe --version` y comparar.
  - [ ] T13.2 Forzar `Cpu` y `Vulkan`, y repetir.
  - [ ] T13.3 "Usar binario existente…" con el binario instalado de T13.1 → `externo`.
  - [ ] T13.4 macOS Apple Silicon: igual que T13.1 (comprobar que no aparece el aviso de Gatekeeper).
  - [ ] T13.5 Anotar los resultados (tag, backend, tiempos aproximados) en este plan.
- **Tests unitarios:** no aplica.

### T14 — Revisión final y cierre (O + reviewer)
- **Depende de:** T1–T13
- **Subtareas:**
  - [ ] T14.1 Reviewer con el diff completo y `última tarea: sí` → `flush` y, con `APPROVED`, `close`.
  - [ ] T14.2 `node scripts/memory.mjs check` y `node scripts/quality-gate.mjs` en verde → `Estado: hecho`.

## Fuera de alcance

- Gestor de descargas completo (cola, reanudación con `Range`, pausa, varias descargas a la vez, comprobación de espacio en disco): **plan 006**. Aquí solo `SimpleHttpFetcher` detrás de `ArtifactFetcher`.
- Lanzar y parar `llama-server`, perfiles, logs y health check: plan del servidor. Este plan solo expone `active_server_path()`.
- Backends SYCL, ROCm/HIP, OpenVINO, OpenCL/Adreno y KleidiAI. Linux y Android.
- Elegir una build concreta (anterior a la última) o volver a la versión previa (rollback).
- Quitar la cuarentena o modificar de cualquier forma un binario **externo**.
- Firma y notarización de la propia app (Fase 7).
- Token de GitHub para ampliar el límite de peticiones.
- Comprobación automática de actualizaciones al arrancar o en segundo plano: solo bajo demanda ("Buscar actualizaciones").

## Riesgos

| Riesgo | Mitigación |
|---|---|
| llama.cpp vuelve a cambiar los nombres de los assets o el esquema de releases (ya pasó con `v1.1.0` en `/latest`) | Clasificación por tokens con fixture actual + fixture antiguo. Selección por `^b\d+$` en `/releases`, no en `/latest`. Un error claro `NoCompatibleRelease` y la alternativa "Usar binario existente…" |
| Límite de 60 peticiones/hora sin token | Solo se consulta bajo demanda (1 petición por consulta). `RateLimited` con la hora de reinicio y un mensaje en español |
| Compatibilidad de versión menor de CUDA (asset 13.5 con driver 13.0) falla en algún driver | Verificación con `--version` antes de activar. Si falla, se conserva la versión anterior y el usuario puede forzar `Vulkan` o `Cpu` |
| Descargas grandes (CUDA + cudart ≈ 600 MB) sin reanudación | Progreso y cancelación. La reanudación llega con el plan 006 por `ArtifactFetcher` |
| `--version` de un build CUDA tarda o se cuelga al inicializar la GPU | Timeout de 15 s en `TokioCommandRunner` y `kill_on_drop` |
| Gatekeeper bloquea `llama-server` aunque se quite la cuarentena (binarios sin firma ad hoc en arm64) | Verificación manual en T13.4. Si falla, `VerificationFailed` con el stderr y se abre una pregunta para firmar ad hoc (`codesign -s -`) en un plan aparte |
| Windows bloquea el borrado del directorio anterior (servidor en marcha, antivirus) | `pending_cleanup` y reintento en `status()`. Se instala en un directorio nuevo por tag, nunca encima del activo |
| Archivo malicioso o corrupto (zip slip) | SHA256 del `digest` de GitHub antes de extraer + extracción con rutas encerradas en `dest` (CA6) |
| `nvidia-smi` no está en el `PATH` o su formato cambia | Si falla, se trata como "sin NVIDIA" sin error. Parsers tolerantes con fixtures. El usuario puede forzar `Cuda` solo si se detectó; si no, `Vulkan` o `Cpu` |
| Choque con el plan 003 en las rutas o en las versiones de dependencias | El plan nombra módulos, no rutas de UI de 003. Se reutilizan las versiones ya fijadas en `Cargo.lock` |

## Preguntas abiertas

1. **Orden de `Auto`:** se propone CUDA > Metal > Vulkan > CPU. ¿Confirmas que con NVIDIA y un driver compatible se prefiere CUDA aunque la descarga sea unas 15 veces mayor (≈ 600 MB frente a ≈ 40 MB de Vulkan)?
2. **Persistencia:** el estado del runtime va en su propio fichero `<app_data>/runtime/runtime.json`. Si otro plan (ajustes o `config`) define un almacén común, ¿se migra allí o se mantiene separado?
3. **Versión anterior:** se borra tras una actualización verificada. ¿Prefieres conservar la anterior para poder volver atrás (ocupa entre 40 y 600 MB más)?
4. **Binario externo en macOS con cuarentena:** el plan no lo modifica y muestra el error de `--version`. ¿Quieres un botón explícito "Quitar cuarentena" (con confirmación) para binarios externos?
5. **Windows ARM64:** la selección lo soporta gratis por patrones, pero no hay máquina para verificarlo. ¿Se declara "no soportado oficialmente" en el MVP?

# 003 — Esqueleto: workspace Rust, app Tauri 2 + Svelte 5 y CI

- **Tipo:** chore
- **Estado:** en curso
- **Rama:** chore/003-esqueleto
- **Fase del roadmap:** 0 (Esqueleto) de [docs/MVP.md](../MVP.md)

## Objetivo

Que el usuario pueda abrir la app de escritorio en Windows y macOS y moverse por sus cinco secciones vacías (Explorar · Biblioteca · Ejecutar · Chat · Ajustes). Así cada fase 1–7 parte de una base que compila, se prueba y pasa la CI en las dos plataformas.

## Contexto

### Qué hay hoy

- No hay código de producto: ni `Cargo.toml`, ni `ui/`, ni `src-tauri/`. El repo solo contiene la governance (planes 000–002).
- `scripts/quality-gate.mjs` ya activa los pasos de Rust si existe `Cargo.toml` (fmt, clippy `-D warnings`, test `--locked`) y los de UI si existe `ui/package.json` (`lint` y `test` obligatorios, `check` opcional). Los pasos se montan en `buildSteps()`, que no se exporta ni tiene tests.
- `.github/workflows/ci.yml` ejecuta el gate en `windows-latest` y `macos-latest`. Ya instala la toolchain si existe `rust-toolchain.toml`, usa `Swatinem/rust-cache@v2` si existe `Cargo.toml` y hace `npm ci --prefix ui` si existe `ui/package-lock.json`. No compila la app.
- El escaneo de patrones prohibidos (R2) y de secretos (R6) ya cubre `crates/`, `src-tauri/src` y `ui/src`.
- `.gitignore` ya ignora `target/`, `node_modules/` y `dist/`.

### Decisiones tomadas (usuario)

- **D1:** Tauri 2 + Svelte 5 + TypeScript con Vite en `ui/`. Workspace Cargo con `crates/core` (sin dependencia de Tauri) y `src-tauri/`.
- **D2:** el chat (fase 6) entra en el MVP, y llama.cpp se descarga o se elige un binario existente. En este plan solo afecta a que existan la sección **Chat** y los módulos `runtime` y `server`, vacíos.

### Decisiones técnicas de este plan

- **D3, nombre del crate:** el directorio es `crates/core`, pero el paquete se llama `app-llama-core` (lib `app_llama_core`). Un crate llamado `core` choca con el `core` de la biblioteca estándar de Rust.
- **D4, app Tauri:** el paquete `src-tauri` se llama `app-llama` (lib `app_llama_lib` + bin `app-llama`, como en la plantilla oficial), con `crate-type = ["rlib"]`: sin móvil, así compila más rápido.
- **D5, edición y resolver:** `edition = "2024"` y `resolver = "3"` en el workspace. La versión `0.1.0` se comparte con `[workspace.package]`.
- **D6, toolchain:** `rust-toolchain.toml` con `channel = "1.100.0"` (estable del 2026-11-12) y `components = ["rustfmt", "clippy"]`. El MSRV de `tauri` 2.12 es 1.90.
- **D7, navegación sin router:** un estado `$state` en `App.svelte` y un módulo TS puro `ui/src/lib/navigation.ts` con las secciones. Sin SvelteKit ni router: es una única ventana sin URLs.
- **D8, comando de ejemplo `app_info`:** devuelve `{ name: "app-llama", version: "0.1.0" }`, construido por `app_llama_core::about::app_info()`. El comando es un envoltorio fino. La sección Ajustes muestra `Versión 0.1.0`.
- **D9, UI compilada antes que Rust en el gate:** `tauri::generate_context!()` incrusta `frontendDist` (`../ui/dist`) al compilar, y la compilación falla si esa carpeta no existe. Por eso el gate añade el paso **Build UI** (`npm --prefix ui run build`) **antes** de los pasos de Rust, siempre que `ui/package.json` defina `build`.
- **D10, `npm --prefix ui run tauri`:** la CLI de Tauri busca `tauri.conf.json` hacia abajo desde el directorio actual. El script es `"tauri": "cd .. && tauri"`: `cd ..` funciona en `cmd` y en `sh`, y npm añade `ui/node_modules/.bin` (ruta absoluta) al `PATH`.

### Versiones verificadas (2026-11-26)

| Paquete | Versión | Fuente |
|---|---|---|
| `tauri` (crate) | 2.12.7, MSRV 1.90 | crates.io API |
| `@tauri-apps/cli` · `@tauri-apps/api` | 2.12.6 · 2.12.5 | registry.npmjs.org |
| `svelte` | 5.59.2 | registry.npmjs.org |
| `vite` | 8.4.0 (Node `^20.19 \|\| >=22.12`) | registry.npmjs.org |
| `@sveltejs/vite-plugin-svelte` | 7.3.2 (peer `vite ^8`, `svelte ^5.46.4`) | registry.npmjs.org |
| `vitest` | 5.1.2 (Node `^22.12 \|\| ^24`, peer `vite ^6.4 \|\| ^7 \|\| ^8`) | registry.npmjs.org |
| Rust estable | 1.100.0 (2026-11-12) | blog.rust-lang.org |

Las versiones exactas quedan fijadas por `Cargo.lock` y `ui/package-lock.json` (R3). Se instalan los mismos *major* de la tabla; el *minor/patch* puede ser el más reciente.

### APIs de test verificadas

- **Rust:** `tauri::test` (feature `test`, módulo marcado como inestable): `mock_builder()`, `mock_context(noop_assets())`, `WebviewWindowBuilder::new(&app, "main", Default::default())` y `get_ipc_response(&webview, InvokeRequest { cmd, callback: CallbackFn(0), error: CallbackFn(1), url: "http://tauri.localhost", body: InvokeBody::default(), headers: Default::default(), invoke_key: INVOKE_KEY.to_string() })`. Fuente: <https://docs.rs/tauri/latest/tauri/test/index.html>.
- **UI:** `mockIPC((cmd, args) => …)` y `clearMocks()` de `@tauri-apps/api/mocks`, con Vitest. jsdom puede necesitar `crypto.getRandomValues`. Fuente: <https://v2.tauri.app/develop/tests/mocking/>.
- Prerrequisitos de Tauri: en Windows, MSVC Build Tools ("Desktop development with C++") y WebView2, ya incluido en Windows 10 1803+. En macOS, Xcode o Command Line Tools. Fuente: <https://v2.tauri.app/start/prerequisites/>.

### Estructura objetivo

```
Cargo.toml                 # workspace virtual: crates/core, src-tauri
Cargo.lock
rust-toolchain.toml
crates/core/
  Cargo.toml               # app-llama-core, sin tauri
  src/lib.rs               # pub mod about, config, download, gguf, hf, runtime, server
  src/about.rs             # AppInfo + app_info()
  src/{hf,download,gguf,runtime,server,config}/mod.rs   # solo //! doc
src-tauri/
  Cargo.toml  build.rs  tauri.conf.json  capabilities/default.json  icons/  .gitignore
  src/main.rs  src/lib.rs  src/commands.rs
ui/
  package.json  package-lock.json  index.html  vite.config.ts  tsconfig.json  eslint.config.js  svelte.config.js
  src/main.ts  src/App.svelte  src/App.test.ts
  src/lib/navigation.ts  src/lib/navigation.test.ts  src/lib/api.ts  src/lib/api.test.ts
  src/views/{Explore,Library,Run,Chat,Settings}.svelte
docs/DESARROLLO.md
```

## Criterios de aceptación

- **CA1 (workspace y core sin Tauri)** — Dado el workspace, cuando se ejecuta `cargo metadata --format-version 1 --no-deps`, entonces los miembros son exactamente `app-llama-core` y `app-llama`. Y `cargo tree -p app-llama-core -e normal` no contiene ninguna línea con `tauri`.
- **CA2 (módulos del §3)** — Dado `crates/core/src/lib.rs`, cuando se compila, entonces declara `pub mod` para `hf`, `download`, `gguf`, `runtime`, `server` y `config`, cada uno con su `mod.rs` que solo tiene un comentario `//!` con su responsabilidad y la fase en la que se implementa.
- **CA3 (comando de ejemplo)** — Dada la app con su manejador de comandos, cuando se invoca `app_info` por IPC (`get_ipc_response` con `MockRuntime`), entonces la respuesta se deserializa como `{ "name": "app-llama", "version": "0.1.0" }`. Un comando inexistente (`no_existe`) devuelve `Err`.
- **CA4 (navegación)** — Dada la UI montada, cuando se pulsa cada uno de los 5 botones de navegación, entonces se muestra un `<h1>` con el nombre de esa sección, el botón pulsado tiene `aria-current="page"` y los otros cuatro no lo tienen. Al montar, la sección activa es **Explorar**. Lo comprueba `App.test.ts`.
- **CA5 (versión en Ajustes)** — Dado `mockIPC` que responde a `app_info` con `{ name: "app-llama", version: "0.1.0" }`, cuando se abre Ajustes, entonces aparece el texto `Versión 0.1.0`. Si `app_info` falla, aparece `Versión no disponible` y no se lanza ninguna excepción sin capturar.
- **CA6 (quality gate)** — Dado el repo con `Cargo.toml` y `ui/package.json` (con `build`, `lint`, `check` y `test`), cuando se ejecuta `node scripts/quality-gate.mjs`, entonces los pasos se ejecutan en este orden: Build UI, Formato Rust, Lint Rust, Tests Rust, Lint UI, Tipos UI, Tests UI, Tests scripts, Memoria (R11), Patrones prohibidos y Secretos. Y termina en verde. El orden lo fija el test `pasos_orden_completo`.
- **CA7 (CI)** — Dado un PR desde `chore/003-esqueleto`, cuando corre la CI, entonces el job pasa en `windows-latest` y `macos-latest`, con el gate en verde y el paso **Build app** (`npm --prefix ui run tauri -- build --debug --no-bundle`) terminado con código 0.
- **CA8 (arranque real en Windows)** — Dado un Windows con los prerrequisitos de `docs/DESARROLLO.md`, cuando se ejecuta `npm ci --prefix ui` y `npm --prefix ui run tauri -- dev`, entonces se abre una ventana titulada `app-llama`, con las 5 secciones navegables y la versión en Ajustes (verificación manual, T8).
- **CA9 (determinismo, R3)** — Dado el repo, entonces `Cargo.lock`, `ui/package-lock.json` y `rust-toolchain.toml` están versionados, y `ui/dist` y `src-tauri/gen/schemas` no lo están. Además, `npm ci --prefix ui` y `cargo build --workspace --locked` funcionan sin modificar los lockfiles (`git status --porcelain` vacío después).
- **CA10 (governance)** — Dado el diff del plan, entonces no aparece ningún patrón de R2, el script `test` de la UI es `vitest run` (no se queda en modo watch) y la tabla §2 de `docs/GOVERNANCE.md` incluye la fila "Build UI".

## Tareas

> Todas las tareas de código son **(developer)**, con TDD: primero los tests de la tabla (rojo) y después el código (verde). **(O)** = orchestrator. Los ficheros generados (`Cargo.lock`, `package-lock.json`, iconos) no cuentan para el límite de ~300 líneas por tarea.

### T1 — Workspace Rust y crate `core` (developer) ✅ hecha
- **Ficheros/módulos:** `Cargo.toml`, `rust-toolchain.toml`, `Cargo.lock`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/core/src/about.rs`, `crates/core/src/{hf,download,gguf,runtime,server,config}/mod.rs`
- **Depende de:** —
- **Cubre:** CA1, CA2, CA9 (parte Rust)
- **Subtareas:**
  - [x] T1.1 `Cargo.toml` raíz: `[workspace] members = ["crates/core"]` (T5 añade `src-tauri`), `resolver = "3"`, `[workspace.package] version = "0.1.0"`, `edition = "2024"`, `license` vacío (no se publica), y `[workspace.dependencies]` con `serde = { version = "1", features = ["derive"] }` y `serde_json = "1"`.
  - [x] T1.2 `rust-toolchain.toml` según D6, con `profile = "minimal"`.
  - [x] T1.3 `crates/core`: paquete `app-llama-core`, `version.workspace = true`, `edition.workspace = true`, dependencia `serde` y dev-dependencia `serde_json`.
  - [x] T1.4 Módulos vacíos. Cada `mod.rs` solo lleva un `//!`:
    - `hf` (cliente de la API de Hugging Face, fase 2);
    - `download` (cola de descargas, reanudación y progreso, fase 3);
    - `gguf` (lector de metadatos GGUF, fase 4);
    - `runtime` (detección de hardware e instalación o selección de llama.cpp, fase 1);
    - `server` (ciclo de vida de llama-server, fase 5);
    - `config` (perfiles, ajustes y rutas, fases 4–5).
  - [x] T1.5 `about.rs`: `#[derive(Debug, Clone, PartialEq, Eq, Serialize)] pub struct AppInfo { pub name: String, pub version: String }` y `pub fn app_info() -> AppInfo`, con `name = "app-llama"` (constante `APP_NAME`) y `version = env!("CARGO_PKG_VERSION")`.
- **Tests unitarios** (en `about.rs`, `#[cfg(test)]`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `app_info_nombre_de_la_app` | `app_info()` | `name == "app-llama"` |
  | `app_info_version_del_workspace` | `app_info()` | `version == env!("CARGO_PKG_VERSION")` y `== "0.1.0"` |
  | `app_info_serializa_campos` | `serde_json::to_value(app_info())` | `{"name":"app-llama","version":"0.1.0"}` exacto (sin campos extra) |
  | `app_info_es_determinista` | dos llamadas | iguales (`assert_eq!`) |

### T2 — Quality gate: pasos extraídos, testeables y con Build UI (developer) ✅ hecha
- **Ficheros/módulos:** `scripts/lib/gate-steps.mjs` (nuevo), `scripts/lib/gate-steps.test.mjs` (nuevo), `scripts/quality-gate.mjs`, `docs/GOVERNANCE.md` (§2)
- **Depende de:** —
- **Cubre:** CA6, CA10
- **Subtareas:**
  - [x] T2.1 Tests (rojo).
  - [x] T2.2 `export function planSteps({ hasCargo, uiScripts, hasMemoryCli })` → `Array<{ label, cmd } | { label, error }>`. Es una función pura, sin acceso a disco. `uiScripts` es `null` si no existe `ui/package.json`, y si existe, el objeto `scripts`. Orden:
    1. `Build UI` (`npm --prefix ui run build`) si `uiScripts?.build`.
    2. Los 3 de Rust si `hasCargo`, con los comandos actuales sin cambios.
    3. Un `{ label: "Script UI '<x>'", error: "ui/package.json debe definir el script '<x>'" }` por cada `lint` o `test` que falte.
    4. `Lint UI`, `Tipos UI` (si hay `check`) y `Tests UI`.
    5. `Tests scripts` y `Memoria (R11)` si `hasMemoryCli`.
    
    No incluye los escaneos internos (Patrones prohibidos y Secretos), que `quality-gate.mjs` añade siempre al final.
  - [x] T2.3 `quality-gate.mjs`: `buildSteps()` usa `planSteps` y conserva la salida y los códigos de salida actuales. Las etiquetas no cambian.
  - [x] T2.4 `GOVERNANCE.md` §2: fila `Build UI | npm --prefix ui run build | script build`, la primera de la tabla, y una nota de una línea: "va antes de Rust porque `src-tauri` incrusta `ui/dist` al compilar".
- **Tests unitarios** (`scripts/lib/gate-steps.test.mjs`, `node:test`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `pasos_repo_vacio` | `{ hasCargo: false, uiScripts: null, hasMemoryCli: false }` | `[]` |
  | `pasos_solo_scripts` | `hasMemoryCli: true`, resto vacío | etiquetas `["Tests scripts", "Memoria (R11)"]` |
  | `pasos_solo_rust` | `hasCargo: true` | `["Formato Rust", "Lint Rust", "Tests Rust"]` con los comandos exactos actuales (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`) |
  | `pasos_orden_completo` | `hasCargo: true`, `uiScripts: { build, lint, check, test }`, `hasMemoryCli: true` | `["Build UI", "Formato Rust", "Lint Rust", "Tests Rust", "Lint UI", "Tipos UI", "Tests UI", "Tests scripts", "Memoria (R11)"]` |
  | `pasos_ui_sin_build` | `uiScripts: { lint, test }` | sin `Build UI` ni `Tipos UI`; `["Lint UI", "Tests UI"]` |
  | `pasos_ui_falta_lint_y_test` | `uiScripts: {}` | dos pasos con `error` que contienen `'lint'` y `'test'`, y ningún `cmd` de UI |
  | `pasos_build_ui_comando` | `uiScripts: { build, lint, test }` | el primer paso es `{ label: "Build UI", cmd: "npm --prefix ui run build" }` |

### T3 — Proyecto UI: Vite + Svelte 5 + TS, tooling y navegación (developer) ✅ hecha
- **Ficheros/módulos:** `ui/package.json`, `ui/package-lock.json`, `ui/index.html`, `ui/vite.config.ts`, `ui/svelte.config.js`, `ui/tsconfig.json`, `ui/eslint.config.js`, `ui/src/main.ts`, `ui/src/lib/navigation.ts`, `ui/src/lib/navigation.test.ts`, `ui/src/vite-env.d.ts`
- **Depende de:** —
- **Cubre:** CA4 (parte de datos), CA9 (parte UI), CA10
- **Subtareas:**
  - [x] T3.1 `package.json` (`"private": true`, `"type": "module"`, `engines.node: ">=22.12"`). Scripts:
    - `dev: vite`
    - `build: vite build`
    - `lint: eslint .`
    - `check: svelte-check --tsconfig ./tsconfig.json --fail-on-warnings`
    - `test: vitest run`
    - `tauri: cd .. && tauri` (D10)

    Dependencias: `svelte` 5 y `@tauri-apps/api` 2. Dependencias de desarrollo:
    - `vite` 8 y `@sveltejs/vite-plugin-svelte` 7;
    - `typescript`, `svelte-check` y `@tsconfig/svelte`;
    - `vitest` 5, `jsdom`, `@testing-library/svelte` y `@testing-library/jest-dom`;
    - `eslint` (flat config), `@eslint/js`, `typescript-eslint`, `eslint-plugin-svelte` y `globals`;
    - `@tauri-apps/cli` 2.
  - [x] T3.2 `vite.config.ts`:
    - `server.port = 5173`, `strictPort: true` y `clearScreen: false` (requisitos de Tauri);
    - `build.outDir = "dist"`;
    - bloque `test` con `environment: "jsdom"`, `setupFiles` para `jest-dom` y `resolve.conditions: ["browser"]` en test, necesario para montar componentes Svelte 5 en Vitest.
  - [x] T3.3 `eslint.config.js` con las reglas recomendadas de JS, TS y Svelte. Ignora `dist/` y `node_modules/`. Sin desactivar reglas (R2).
  - [x] T3.4 `navigation.ts`:
    - `export const SECTIONS = [{ id: "explore", label: "Explorar" }, { id: "library", label: "Biblioteca" }, { id: "run", label: "Ejecutar" }, { id: "chat", label: "Chat" }, { id: "settings", label: "Ajustes" }] as const`;
    - `export type SectionId`;
    - `export const INITIAL_SECTION: SectionId = "explore"`;
    - `export function isSectionId(value: string): value is SectionId`;
    - `export function sectionLabel(id: SectionId): string`.
  - [x] T3.5 `main.ts` con `mount(App, { target: document.getElementById("app")! })`. Como `App.svelte` llega en T4, aquí se crea un `App.svelte` mínimo (un `<main>` vacío) para que `build` y `check` pasen.
- **Tests unitarios** (`navigation.test.ts`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `secciones_en_orden` | `SECTIONS.map(s => s.id)` | `["explore","library","run","chat","settings"]` |
  | `secciones_con_etiqueta_en_espanol` | `SECTIONS.map(s => s.label)` | `["Explorar","Biblioteca","Ejecutar","Chat","Ajustes"]` |
  | `seccion_inicial_es_explorar` | `INITIAL_SECTION` | `"explore"` |
  | `ids_de_seccion_validos` | cada id de `SECTIONS` | `isSectionId(id) === true` |
  | `ids_de_seccion_invalidos` | `""`, `"Explore"`, `"ajustes"`, `"constructor"`, `"toString"` | `false` en todos (sin buscar en el prototipo) |
  | `etiqueta_de_seccion` | `sectionLabel("chat")` | `"Chat"` |

### T4 — Shell de la UI: navegación y vistas vacías (developer) ✅ hecha
- **Ficheros/módulos:** `ui/src/App.svelte`, `ui/src/App.test.ts`, `ui/src/views/{Explore,Library,Run,Chat,Settings}.svelte`, `ui/src/app.css`
- **Depende de:** T3
- **Cubre:** CA4
- **Subtareas:**
  - [x] T4.1 Tests (rojo).
  - [x] T4.2 `App.svelte`:
    - `let active = $state<SectionId>(INITIAL_SECTION)`;
    - `<nav aria-label="Secciones">` con un `<button>` por cada sección de `SECTIONS`, y `aria-current="page"` solo en la activa;
    - `<main>` que muestra la vista activa.
  - [x] T4.3 Cada vista es un componente con `<h1>{etiqueta}</h1>` y un párrafo "Próximamente". `Settings.svelte` lleva un hueco para la versión, que se rellena en T6.
  - [x] T4.4 `app.css`: estilos mínimos (barra lateral y contenido), sin librería de componentes.
- **Tests unitarios** (`App.test.ts`, `@testing-library/svelte`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `muestra_cinco_botones_de_navegacion` | `render(App)` | 5 `button` dentro de `navigation "Secciones"`, con los textos de `SECTIONS` en orden |
  | `seccion_inicial_explorar` | `render(App)` | `heading level 1` = "Explorar"; botón "Explorar" con `aria-current="page"` |
  | `navega_a_cada_seccion` | click en cada botón (bucle sobre `SECTIONS`) | `h1` = etiqueta pulsada; exactamente 1 botón con `aria-current="page"` |
  | `click_en_la_activa_no_cambia` | click en "Explorar" estando en Explorar | sigue en Explorar, sin errores |

### T5 — App Tauri y comando `app_info` (developer)
- **Ficheros/módulos:**
  - `Cargo.toml` (añade `src-tauri` a `members`, más `tauri` y `tauri-build` a `[workspace.dependencies]`);
  - `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/icons/*`, `src-tauri/.gitignore`;
  - `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`;
  - `Cargo.lock`.
- **Depende de:** T1, T3 (`ui/dist` y la CLI)
- **Cubre:** CA1, CA3, CA9
- **Subtareas:**
  - [ ] T5.1 Tests (rojo).
  - [ ] T5.2 `src-tauri/Cargo.toml`:
    - paquete `app-llama` (D4) con `[lib] name = "app_llama_lib"`, `crate-type = ["rlib"]`;
    - dependencias `tauri` 2, `serde` y `app-llama-core = { path = "../crates/core" }`;
    - `[build-dependencies] tauri-build` 2;
    - `[dev-dependencies] tauri = { …, features = ["test"] }` y `serde_json`.
  - [ ] T5.3 `tauri.conf.json` (schema v2):
    - `productName: "app-llama"`, sin `version` (se toma de Cargo);
    - `identifier: "io.github.devlitus.appllama"` (P1, decidido por el usuario);
    - `build.frontendDist: "../ui/dist"`, `build.devUrl: "http://localhost:5173"`, `beforeDevCommand: "npm --prefix ui run dev"` y `beforeBuildCommand: "npm --prefix ui run build"` (si la CLI no los ejecuta desde la raíz, se usa la forma `{ script, cwd }`; ver Riesgos);
    - una ventana `main` con `title: "app-llama"` y 1200×800;
    - `bundle.active: false` (el empaquetado es la fase 7).
  - [ ] T5.4 `capabilities/default.json` con `windows: ["main"]` y `permissions: ["core:default"]`. Los comandos propios de la app no necesitan permiso.
  - [ ] T5.5 Iconos: los de la plantilla oficial de `create-tauri-app` (generados con `npm --prefix ui run tauri -- icon <png>`). Son provisionales hasta la fase 7. `src-tauri/.gitignore` ignora `/target/` y `/gen/schemas`.
  - [ ] T5.6 `commands.rs`: `#[tauri::command] pub fn app_info() -> AppInfo { app_llama_core::about::app_info() }`.
  - [ ] T5.7 `lib.rs`:
    - `pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R>`, que añade `invoke_handler(tauri::generate_handler![commands::app_info])`;
    - `pub fn run() -> Result<(), tauri::Error>`, que hace `register(tauri::Builder::default()).run(tauri::generate_context!())`.
    
    `main.rs` lleva `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` y, si `run()` devuelve error, escribe el error en stderr y sale con `std::process::exit(1)`. Nada de `unwrap()` ni `expect()` fuera de tests (R7).
- **Tests unitarios** (`src-tauri/src/lib.rs` y `commands.rs`, `#[cfg(test)]`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `comando_app_info_delega_en_core` | `commands::app_info()` | `== app_llama_core::about::app_info()` |
  | `ipc_app_info_responde` | `register(tauri::test::mock_builder())` + `mock_context(noop_assets())` + `get_ipc_response` con `cmd: "app_info"` | `Ok`, deserializa a `serde_json::Value` `{"name":"app-llama","version":"0.1.0"}` |
  | `ipc_comando_desconocido_falla` | igual, con `cmd: "no_existe"` | `Err` |

### T6 — UI ↔ backend: versión en Ajustes (developer)
- **Ficheros/módulos:** `ui/src/lib/api.ts`, `ui/src/lib/api.test.ts`, `ui/src/views/Settings.svelte`, `ui/src/views/Settings.test.ts`
- **Depende de:** T4, T5
- **Cubre:** CA3 (lado UI), CA5
- **Subtareas:**
  - [ ] T6.1 Tests (rojo). `afterEach(clearMocks)`. Si jsdom no trae `crypto.getRandomValues`, se añade un polyfill determinista en el `setupFiles`.
  - [ ] T6.2 `api.ts`: `export interface AppInfo { name: string; version: string }` y `export function getAppInfo(): Promise<AppInfo>`, que llama a `invoke<AppInfo>("app_info")`. Es el único punto de la UI que conoce el nombre del comando.
  - [ ] T6.3 `Settings.svelte`: al montarse llama a `getAppInfo()`. Mientras carga muestra `Versión …`, si acaba bien `Versión {version}` y si falla `Versión no disponible`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `get_app_info_invoca_app_info` | `mockIPC` que guarda `cmd` y responde `{name:"app-llama",version:"0.1.0"}` | resuelve ese objeto; `cmd === "app_info"`, una sola llamada |
  | `get_app_info_propaga_error` | `mockIPC` que lanza `new Error("ipc")` | la promesa se rechaza |
  | `ajustes_muestra_version` | `render(Settings)` con el mock OK | `findByText("Versión 0.1.0")` |
  | `ajustes_version_no_disponible` | `render(Settings)` con el mock que lanza | `findByText("Versión no disponible")`, sin rechazos sin capturar |

### T7 — CI con build de la app y guía de desarrollo (developer)
- **Ficheros/módulos:** `.github/workflows/ci.yml`, `docs/DESARROLLO.md` (nuevo)
- **Depende de:** T2, T5, T6
- **Cubre:** CA7, CA9
- **Subtareas:**
  - [ ] T7.1 `ci.yml`: añadir tras "Quality gate" el paso `Build app` → `npm --prefix ui run tauri -- build --debug --no-bundle`. Los pasos de toolchain, cache y `npm ci` actuales no cambian: ya se activan solos al existir los ficheros. `Swatinem/rust-cache@v2` se mantiene en la raíz del workspace.
  - [ ] T7.2 Comprobar en local (Windows) que el paso de R10 de la CI acepta `chore/003-esqueleto`.
  - [ ] T7.3 `docs/DESARROLLO.md`:
    - prerrequisitos por SO (enlace a la página oficial de Tauri), Node ≥ 22.12 y Rust gestionado por `rust-toolchain.toml`;
    - comandos: `npm ci --prefix ui`, `npm --prefix ui run tauri -- dev`, `node scripts/quality-gate.mjs` y `npm --prefix ui run tauri -- build --debug --no-bundle`;
    - una nota: el gate compila la UI antes de Rust (D9).
- **Tests unitarios:** no aplica (configuración de CI y documentación). La verificación es CA7, en el PR.

### T8 — Verificación en vivo, revisión final y cierre (O + reviewer)
- **Depende de:** T1–T7
- **Cubre:** CA7, CA8, y todos en la revisión
- **Subtareas:**
  - [ ] T8.1 (O) En Windows: `npm ci --prefix ui`, `npm --prefix ui run tauri -- dev`. Se comprueba la ventana, la navegación por las 5 secciones y `Versión 0.1.0` en Ajustes. Después, `git status --porcelain` vacío (CA9). Los resultados se anotan en este plan.
  - [ ] T8.2 (O) macOS: solo la CI (CA7), mientras no haya un Mac (MVP §7.4). Se anota en el plan.
  - [ ] T8.3 El orchestrator lanza el `reviewer` con el plan, el diff completo y `última tarea: sí`. El reviewer hace `session`, `flush` y, con `APPROVED`, `close`. Como máximo, 3 ciclos.
  - [ ] T8.4 `node scripts/memory.mjs check` y `node scripts/quality-gate.mjs` en verde → `Estado: hecho`.
- **Tests unitarios:** no aplica (los automatizados son T1–T6).

## Fuera de alcance

- Toda la lógica de las fases 1–7: los módulos de `core` quedan vacíos y las vistas solo muestran "Próximamente".
- Empaquetado, instaladores `.msi`/`.dmg`, firma, notarización e iconos definitivos (fase 7). En este plan, `bundle.active: false`.
- Linux, móvil (iOS/Android) y auto-update.
- Router o SvelteKit, i18n, tema claro/oscuro y librería de componentes.
- Plugins de Tauri (shell, fs, dialog…): se añadirán en la fase que los necesite.
- Tests e2e con WebDriver (`@wdio/tauri-service`).
- Prettier u otro formateador de la UI: no está en el gate de la governance.

## Riesgos

| Riesgo | Mitigación |
|---|---|
| `generate_context!()` falla si `ui/dist` no existe, y clippy y los tests de Rust caen en un checkout limpio | D9: el paso `Build UI` va antes de Rust en el gate (test `pasos_orden_completo`). La CI hace `npm ci` antes del gate. |
| La primera compilación de Tauri tarda varios minutos y el hook `Stop` (timeout de 900 s) o el `pre-commit` se hacen lentos | `Swatinem/rust-cache` en CI. En local, `target/` se reutiliza y las compilaciones incrementales tardan segundos. `crate-type = ["rlib"]` evita compilar `cdylib`/`staticlib`. Si se supera el timeout, se escala al usuario. |
| La CLI de Tauri no ejecuta `beforeDevCommand`/`beforeBuildCommand` desde la raíz del repo | CA7 lo comprueba en CI (`tauri build` ejecuta `beforeBuildCommand`). Si falla, se usa la forma `{ "script": "npm run build", "cwd": "../ui" }`, comprobando la base del `cwd` en el código de la CLI. |
| `tauri::test` es un módulo inestable y su API puede cambiar entre *minor* | Se usa la forma de docs.rs verificada el 2026-11-26. La versión queda fijada por `Cargo.lock`. Los tests de IPC solo dependen de `register`, que es la función de producción. |
| Montar componentes Svelte 5 en Vitest con jsdom resuelve la build de servidor | `resolve.conditions: ["browser"]` en la configuración de test (T3.2). |
| jsdom no trae `crypto.getRandomValues`, que usa `mockIPC` | Polyfill en `setupFiles` con `node:crypto` (`webcrypto`). Es determinista para los tests porque no se comparan valores aleatorios. |
| Un crate llamado `core` choca con `core` de std | D3: el paquete es `app-llama-core`. |
| Sin Mac no se prueba la app en macOS más allá de compilar | La CI compila y pasa los tests en `macos-latest` (CA7). La prueba manual queda pendiente (MVP §7.4). |
| El `script tauri` con `cd ..` se comporta distinto en `cmd` y en `sh` | `cd .. && x` es válido en los dos. Lo comprueban T8.1 (Windows) y CA7 (macOS, en CI). |

## Preguntas abiertas

- ~~P1 — Identificador del bundle.~~ **Resuelta (2026-11-26):** `io.github.devlitus.appllama`. No se cambia después de la primera release.
- ~~P2 — Nombre visible de la app.~~ **Resuelta (2026-11-26):** `app-llama`.
- **Nota (2026-11-26):** de momento no hay pruebas manuales en Mac; macOS se valida solo con la CI (T8.2).

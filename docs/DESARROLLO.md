# Guía de desarrollo

## Prerrequisitos

- **Node.js ≥ 22.12** (la CI usa Node 24).
- **Rust**: la versión la fija `rust-toolchain.toml`; `rustup` la instala sola al ejecutar `cargo` en el repositorio.
- **Dependencias del sistema de Tauri 2** según el SO. Referencia oficial: <https://v2.tauri.app/start/prerequisites/>
  - **Windows**: Microsoft C++ Build Tools (Visual Studio Build Tools) con el componente **VC Tools** (carga de trabajo "Desarrollo de escritorio con C++") y WebView2 (ya incluido en Windows 10/11 recientes).
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`).

## Comandos

Todos se ejecutan desde la raíz del repositorio.

| Qué | Comando |
|---|---|
| Instalar dependencias de la UI | `npm ci --prefix ui` |
| Ejecutar la app en desarrollo | `npm --prefix ui run tauri -- dev` |
| Quality gate completo | `node scripts/quality-gate.mjs` |
| Compilar la app (sin instalador) | `npm --prefix ui run tauri -- build --debug --no-bundle` |

## Notas

- El quality gate compila la UI (`ui/dist`) **antes** de compilar Rust: `tauri::generate_context!` necesita `frontendDist` existente en tiempo de compilación.
- La CSP de `src-tauri/tauri.conf.json` es estricta (`style-src 'self'`, sin `unsafe-inline`): la UI no debe usar estilos en línea ni `data:` en imágenes; los estilos van en ficheros CSS importados.
- La CSP solo se aplica a lo que Tauri sirve desde los recursos incrustados (compilación de producción). En `tauri dev` la página la sirve Vite desde `devUrl`, así que esa CSP no se aplica y los estilos que Vite inyecta con `<style>` funcionan. Por eso no hay `devCsp`.

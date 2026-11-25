# Plan MVP — Gestor de modelos para llama.cpp

Aplicación de escritorio multiplataforma (Windows y macOS), escrita en Rust, para **descargar modelos GGUF desde Hugging Face, configurarlos y ejecutarlos con llama.cpp**.

---

## 1. Stack recomendado

| Capa | Elección | Motivo |
|---|---|---|
| Framework de escritorio | **Tauri 2** | Backend Rust nativo, UI web, instaladores ligeros (~10 MB) para Windows (.msi/.exe) y macOS (.dmg); trae bundler, firma y auto-update |
| Frontend | **Svelte + TypeScript** (o React) | Iteración rápida. Alternativa 100 % Rust: **Leptos** dentro de Tauri (más lento de desarrollar) |
| HTTP / descargas | `reqwest` + `tokio` | Streaming, reanudación con cabecera `Range`, progreso |
| Hugging Face | API REST directa (`/api/models`, `/api/models/{id}/tree/main`) | Más control que el crate `hf-hub` para filtrar GGUF y reanudar descargas |
| Lectura de GGUF | Parser propio mínimo (cabecera + metadatos) | Mostrar arquitectura, contexto, parámetros y cuantización sin cargar el modelo |
| Procesos | `tokio::process` | Lanzar `llama-server` y capturar stdout/stderr en vivo |
| Persistencia | TOML/JSON en el directorio de datos de la app (crate `directories`) | Suficiente para el MVP; SQLite más adelante si hace falta |
| Secretos | Crate `keyring` | Token de HF en el llavero del sistema (Credential Manager / Keychain) |

> **Alternativa nativa pura:** `egui` o `Slint`. Más fácil de empaquetar, pero UI menos pulida y más trabajo para tablas, búsqueda y chat. Se recomienda Tauri.

---

## 2. Funcionalidades del MVP

### A. Gestión de llama.cpp
- Detección de hardware: CPU (AVX2/AVX512), GPU NVIDIA (CUDA), Apple Silicon (Metal), Vulkan.
- Descarga automática de la release precompilada adecuada desde GitHub (`ggml-org/llama.cpp/releases`), o selección de un binario ya instalado.
- Mostrar la versión instalada y permitir actualizarla.

### B. Explorador de Hugging Face
- Búsqueda de modelos filtrando por `gguf`, ordenados por descargas o likes.
- Ficha del modelo: lista de archivos `.gguf` con cuantización (Q4_K_M, Q8_0…), tamaño y estimación de RAM/VRAM.
- Descarga con barra de progreso, pausa/reanudación y cancelación, incluidos los **modelos divididos** (`-00001-of-0000N.gguf`).
- Token de HF para modelos con acceso restringido (gated).

### C. Biblioteca local
- Lista de modelos descargados con sus metadatos GGUF.
- Borrar modelos, abrir su carpeta y elegir el directorio de modelos.
- Importar un `.gguf` que ya exista en disco.

### D. Ejecución y configuración
- Perfil por modelo con los parámetros principales: `ctx-size`, `n-gpu-layers`, `threads`, `batch-size`, puerto, `flash-attn`, plantilla de chat, temperatura y top-p.
- Botones de iniciar y parar `llama-server`, con estado visible (cargando / listo / error).
- Consola de logs en tiempo real.
- URL del endpoint compatible con OpenAI (`http://127.0.0.1:8080/v1`) para usarlo desde otras apps.

### E. Chat mínimo (opcional, recomendado)
- Chat sencillo con streaming contra el servidor local, para validar que el modelo funciona sin salir de la app.

---

## 3. Arquitectura

```
app-llama/
├─ crates/
│  └─ core/                 # lógica pura, testeable sin UI
│     ├─ hf/                # cliente API Hugging Face
│     ├─ download/          # gestor de descargas (cola, reanudación, progreso)
│     ├─ gguf/              # lector de metadatos
│     ├─ runtime/           # detección HW + instalación de llama.cpp
│     ├─ server/            # ciclo de vida de llama-server
│     └─ config/            # perfiles, ajustes, rutas
├─ src-tauri/               # comandos Tauri + eventos (progreso, logs)
├─ ui/                      # Svelte: Explorar · Biblioteca · Ejecutar · Chat · Ajustes
└─ docs/
```

- El crate `core` **no depende de Tauri**, así se puede probar de forma aislada.
- La UI llama a ese código mediante `#[tauri::command]`.
- El backend envía el progreso de las descargas y los logs a la UI como **eventos**.

---

## 4. Fases

| Fase | Contenido | Estimación |
|---|---|---|
| **0. Esqueleto** | Proyecto Tauri, workspace Rust, CI en GitHub Actions (build Windows + macOS) | 1–2 días |
| **1. Runtime** | Detección de hardware, descarga y extracción de llama.cpp, verificación con `--version` | 2–3 días |
| **2. Hugging Face** | Búsqueda, ficha del modelo, listado de GGUF, token | 2–3 días |
| **3. Descargas** | Cola, progreso, reanudación, archivos divididos, verificación de tamaño y SHA256 | 3–4 días |
| **4. Biblioteca + GGUF** | Escaneo del directorio, parser de metadatos, borrado e importación | 2 días |
| **5. Ejecución** | Perfiles, lanzar y parar el servidor, logs, health check (`/health`) | 3 días |
| **6. Chat** | UI de chat con streaming (SSE) | 2 días |
| **7. Empaquetado** | Instaladores .msi y .dmg, iconos, firma básica | 2–3 días |

**Total estimado: 3–4 semanas** para una persona.

---

## 5. Riesgos conocidos

- **macOS Gatekeeper:** los binarios de llama.cpp descargados llevan el atributo de cuarentena; hay que quitarlo. Para distribuir la app hay que firmarla y notarizarla (cuenta de Apple Developer, 99 $/año).
- **CUDA en Windows:** las releases de CUDA necesitan los DLL `cudart`, que se publican como un zip aparte en la misma release.
- **Nombres de los assets de llama.cpp:** cambian entre versiones; el detector debe usar patrones, no nombres fijos.
- **Estimación de VRAM:** solo aproximada (tamaño del archivo + KV cache según el contexto); se mostrará como orientativa.
- **Descargas grandes (>50 GB):** obligan a soportar reanudación y a comprobar el espacio en disco antes de empezar.

---

## 6. Fuera del MVP

- Varios servidores a la vez.
- Modelos multimodales (`mmproj`).
- Benchmarks.
- Compilar llama.cpp desde el código fuente.
- Auto-update de la app.
- Linux (Tauri lo soporta casi gratis más adelante).

---

## 7. Decisiones pendientes

1. **Frontend:** Svelte/TS (recomendado), React o 100 % Rust con Leptos.
2. **Chat:** ¿entra en el MVP o basta con lanzar el servidor y mostrar el endpoint?
3. **llama.cpp:** ¿la app lo descarga sola (recomendado) o solo apunta a una instalación existente?
4. **Mac para pruebas:** el build de macOS puede hacerse en GitHub Actions, pero hace falta un Mac para probarlo.

//! Error que los comandos Tauri devuelven a la UI.
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
}

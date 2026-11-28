pub mod downloads;
pub mod error;
pub mod hf;
pub mod runtime;

use app_llama_core::about::AppInfo;

#[tauri::command]
pub fn app_info() -> AppInfo {
    app_llama_core::about::app_info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comando_app_info_delega_en_core() {
        assert_eq!(app_info(), app_llama_core::about::app_info());
    }
}

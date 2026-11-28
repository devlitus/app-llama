use tauri::Manager;

mod commands;

pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        commands::app_info,
        commands::runtime::runtime_hardware,
        commands::runtime::runtime_status,
        commands::runtime::runtime_check_update,
        commands::runtime::runtime_install,
        commands::runtime::runtime_cancel_install,
        commands::runtime::runtime_use_external,
        commands::hf::hf_search,
        commands::hf::hf_search_next,
        commands::hf::hf_model_detail,
        commands::hf::hf_token_status,
        commands::hf::hf_token_save,
        commands::hf::hf_token_delete,
        commands::downloads::downloads_enqueue_hf,
        commands::downloads::downloads_list,
        commands::downloads::downloads_pause,
        commands::downloads::downloads_resume,
        commands::downloads::downloads_cancel,
        commands::downloads::downloads_retry,
        commands::downloads::downloads_remove,
    ])
}

pub fn run() -> Result<(), tauri::Error> {
    register(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = commands::runtime::production_state(&app.path().app_data_dir()?)?;
            app.manage(state);
            let hf = commands::hf::production_state()?;
            let service = hf.service();
            app.manage(hf);
            app.manage(commands::downloads::production_state(
                &app.path().app_local_data_dir()?,
                service,
                app.handle().clone(),
            )?);
            Ok(())
        })
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    use tauri::webview::InvokeRequest;
    use tauri::{
        WebviewWindowBuilder,
        ipc::{CallbackFn, InvokeBody},
    };

    // Tauri sirve la app desde `http://tauri.localhost` solo en Windows y Android;
    // en el resto de plataformas el origen local es `tauri://localhost`.
    #[cfg(any(windows, target_os = "android"))]
    const LOCAL_ORIGIN: &str = "http://tauri.localhost";
    #[cfg(not(any(windows, target_os = "android")))]
    const LOCAL_ORIGIN: &str = "tauri://localhost";

    fn invoke(cmd: &str) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
        let app = register(mock_builder())
            .build(mock_context(noop_assets()))
            .expect("la app de prueba se construye");
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("la ventana de prueba se construye");
        get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: LOCAL_ORIGIN.parse().expect("url válida"),
                body: InvokeBody::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
    }

    #[test]
    fn ipc_app_info_responde() {
        let response = invoke("app_info").expect("app_info responde");
        let value: serde_json::Value = response.deserialize().expect("json válido");
        assert_eq!(
            value,
            serde_json::json!({"name": "app-llama", "version": "0.1.0"})
        );
    }

    #[test]
    fn ipc_comando_desconocido_falla() {
        let error = invoke("no_existe").expect_err("el comando no existe");
        assert_eq!(error, serde_json::json!("Command no_existe not found"));
    }
}

mod commands;

pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![commands::app_info])
}

pub fn run() -> Result<(), tauri::Error> {
    register(tauri::Builder::default()).run(tauri::generate_context!())
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
                url: "http://tauri.localhost".parse().expect("url válida"),
                body: InvokeBody::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
    }

    #[test]
    fn ipc_app_info_responde() {
        let response = invoke("app_info").expect("app_info responde");
        let value: serde_json::Value = response.deserialize().expect("json valido");
        assert_eq!(
            value,
            serde_json::json!({"name": "app-llama", "version": "0.1.0"})
        );
    }

    #[test]
    fn ipc_comando_desconocido_falla() {
        assert!(invoke("no_existe").is_err());
    }
}

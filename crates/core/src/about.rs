use serde::Serialize;

pub const APP_NAME: &str = "app-llama";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

pub fn app_info() -> AppInfo {
    AppInfo {
        name: APP_NAME.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_nombre_de_la_app() {
        assert_eq!(app_info().name, "app-llama");
    }

    #[test]
    fn app_info_version_del_workspace() {
        let info = app_info();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.version, "0.1.0");
    }

    #[test]
    fn app_info_serializa_campos() {
        let value = serde_json::to_value(app_info()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"name": "app-llama", "version": "0.1.0"})
        );
    }

    #[test]
    fn app_info_es_determinista() {
        assert_eq!(app_info(), app_info());
    }
}

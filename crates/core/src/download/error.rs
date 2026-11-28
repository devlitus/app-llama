use super::model::FailureKind;

/// Errores del gestor de descargas. Ningún mensaje incluye cabeceras, URLs
/// firmadas ni el token: solo datos que ya son públicos o locales.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("la descarga ha fallado: {0}")]
    Failed(FailureKind),
    #[error("descarga cancelada")]
    Cancelled,
    #[error("ruta de fichero no permitida")]
    UnsafePath,
    #[error("las partes del modelo dividido no están completas o no son coherentes")]
    IncompleteSplit,
    #[error("petición de descarga no válida: {0}")]
    InvalidRequest(&'static str),
    #[error("el modelo ya está en la cola de descargas")]
    AlreadyQueued,
    #[error("el fichero de destino ya existe")]
    DestinationExists,
    #[error("la orden no es válida en el estado actual de la descarga")]
    InvalidTransition,
    #[error("no existe esa descarga")]
    UnknownDownload,
    #[error("error de E/S: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_sin_token() {
        let secret = format!("hf_{}", "aB3dE6gH9jK2mN5pQ8sT1vW4yZ7cF0hJ3l");
        assert_eq!(secret.len(), 3 + 34);
        let errors = [
            DownloadError::Failed(FailureKind::Network),
            DownloadError::Failed(FailureKind::Unauthorized),
            DownloadError::Failed(FailureKind::Gated),
            DownloadError::Failed(FailureKind::NotFound),
            DownloadError::Failed(FailureKind::ChecksumMismatch),
            DownloadError::Failed(FailureKind::SizeMismatch {
                expected: 1,
                actual: 2,
            }),
            DownloadError::Failed(FailureKind::InsufficientSpace {
                needed: 2,
                available: 1,
            }),
            DownloadError::Failed(FailureKind::UnexpectedContentRange),
            DownloadError::Failed(FailureKind::DestinationExists),
            DownloadError::Failed(FailureKind::Io {
                message: "disco lleno".to_string(),
            }),
            DownloadError::Cancelled,
            DownloadError::UnsafePath,
            DownloadError::IncompleteSplit,
            DownloadError::InvalidRequest("sin partes"),
            DownloadError::AlreadyQueued,
            DownloadError::DestinationExists,
            DownloadError::InvalidTransition,
            DownloadError::UnknownDownload,
            DownloadError::Io(std::io::Error::other("fallo local")),
        ];
        for error in errors {
            assert!(!error.to_string().contains(&secret), "{error:?}");
        }
    }
}

/// Errores del cliente de Hugging Face. Ninguna variante transporta el token.
#[derive(Debug, thiserror::Error)]
pub enum HfError {
    #[error("sin conexión con Hugging Face")]
    Network,
    #[error("token no válido o ausente")]
    Unauthorized,
    #[error("acceso denegado")]
    Forbidden,
    #[error("recurso no encontrado")]
    NotFound,
    #[error("Hugging Face limita las peticiones")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("Hugging Face respondió con el estado HTTP {0}")]
    Status(u16),
    #[error("respuesta inesperada de Hugging Face: {0}")]
    Parse(String),
    #[error("identificador de repositorio no válido")]
    InvalidRepoId,
    #[error("el token no tiene un formato válido")]
    InvalidToken,
    #[error("la paginación superó el límite de páginas")]
    TooManyPages,
    #[error("la URL de paginación apunta a otro host")]
    ForeignUrl,
    #[error("fallo del almacén de secretos: {0}")]
    Secret(String),
}

//! Utilidades compartidas por los tests del gestor de descargas.

use std::path::Path;
use std::sync::Mutex;

use super::auth::TokenProvider;
use super::model::{FileSpec, FileStatus};
use crate::hf::HfToken;

pub fn pattern_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

pub fn file_spec(url: String, dest: &Path, expected_size: usize) -> FileSpec {
    FileSpec {
        relative_path: "m.gguf".to_string(),
        url,
        dest: dest.to_path_buf(),
        expected_size: expected_size as u64,
        sha256: None,
        status: FileStatus::Pending,
    }
}

/// Puerto en el que nadie escucha: el `TcpListener` se suelta al salir.
pub fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("puerto efímero");
    listener.local_addr().expect("dirección local").port()
}

/// Servidor TCP mínimo que puede cortar el cuerpo a mitad, algo que wiremock
/// no sabe hacer. Cada conexión atiende una sola petición.
pub struct CuttingServer {
    port: u16,
    ranges: std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>,
    task: tokio::task::JoinHandle<()>,
}

impl CuttingServer {
    pub fn url(&self) -> String {
        format!("{}/f", self.base_url())
    }

    /// Origen del servidor, para usarlo como base de HF.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Cabecera `Range` de cada petición recibida, en orden de llegada.
    pub fn ranges(&self) -> Vec<Option<String>> {
        self.ranges.lock().expect("registro de Range").clone()
    }
}

impl Drop for CuttingServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// La conexión `n` se corta tras `cut_after[n]` bytes del cuerpo; `None` o
/// una conexión más allá de la lista envían el cuerpo entero. Con
/// `ignore_range` responde siempre `200` desde 0, aunque llegue un `Range`.
pub async fn cutting_server(
    body: Vec<u8>,
    cut_after: Vec<Option<usize>>,
    ignore_range: bool,
) -> CuttingServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("puerto efímero");
    let port = listener.local_addr().expect("dirección local").port();
    let ranges = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = std::sync::Arc::clone(&ranges);
    let task = tokio::spawn(async move {
        let mut connection = 0;
        while let Ok((stream, _)) = listener.accept().await {
            let cut = cut_after.get(connection).copied().flatten();
            connection += 1;
            serve_connection(stream, &body, cut, ignore_range, &log).await;
        }
    });
    CuttingServer { port, ranges, task }
}

async fn serve_connection(
    mut stream: tokio::net::TcpStream,
    body: &[u8],
    cut: Option<usize>,
    ignore_range: bool,
    log: &std::sync::Mutex<Vec<Option<String>>>,
) {
    use tokio::io::AsyncWriteExt;

    let Some(request) = read_request_head(&mut stream).await else {
        return;
    };
    let range = request_range(&request);
    log.lock().expect("registro de Range").push(range.clone());
    let offset = if ignore_range {
        None
    } else {
        range.as_deref().and_then(range_offset)
    };
    let (head, payload) = response_parts(body, offset);
    let sent = cut.map_or(payload.len(), |limit| limit.min(payload.len()));
    // Los fallos de escritura son el cliente cerrando: no son del test.
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(&payload[..sent]).await;
    let _ = stream.shutdown().await;
}

async fn read_request_head(stream: &mut tokio::net::TcpStream) -> Option<String> {
    use tokio::io::AsyncReadExt;

    let mut received = Vec::new();
    let mut buffer = [0u8; 1024];
    while !received.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            return None;
        }
        received.extend_from_slice(&buffer[..read]);
    }
    Some(String::from_utf8_lossy(&received).into_owned())
}

fn request_range(request: &str) -> Option<String> {
    request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("range")
            .then(|| value.trim().to_string())
    })
}

fn range_offset(range: &str) -> Option<usize> {
    range
        .strip_prefix("bytes=")?
        .strip_suffix('-')?
        .parse()
        .ok()
}

fn response_parts(body: &[u8], offset: Option<usize>) -> (String, &[u8]) {
    match offset {
        Some(start) => {
            let payload = &body[start.min(body.len())..];
            let head = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\n\
                 Content-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                payload.len(),
                start,
                body.len().saturating_sub(1),
                body.len()
            );
            (head, payload)
        }
        None => {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            (head, body)
        }
    }
}

/// Token válido cuyo cuerpo empieza por `tag`, para distinguir uno de otro.
pub fn hf_token(tag: char) -> HfToken {
    HfToken::parse(&format!("hf_{tag}{}", "a1B2".repeat(9))).expect("token de prueba")
}

/// Proveedor de tokens en memoria cuyo valor se puede cambiar en mitad de un test.
pub struct FakeTokens(Mutex<Option<HfToken>>);

impl FakeTokens {
    pub fn new(token: Option<HfToken>) -> Self {
        Self(Mutex::new(token))
    }

    pub fn set(&self, token: Option<HfToken>) {
        *self.0.lock().expect("token de prueba") = token;
    }
}

impl TokenProvider for FakeTokens {
    fn token(&self) -> Option<HfToken> {
        self.0.lock().expect("token de prueba").clone()
    }
}

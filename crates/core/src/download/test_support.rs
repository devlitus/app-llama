//! Utilidades compartidas por los tests del gestor de descargas.

use std::path::Path;

use super::model::{FileSpec, FileStatus};

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

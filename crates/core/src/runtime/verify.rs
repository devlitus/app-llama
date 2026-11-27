//! Verificación de `llama-server` con `--version` y limpieza de la cuarentena de macOS.
use std::error::Error;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::runtime::process::hide_console_async;
use crate::runtime::{Os, RuntimeError};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// Cuántos caracteres finales de `stderr` se conservan en los errores.
const STDERR_TAIL_CHARS: usize = 2000;

const XATTR: &str = "/usr/bin/xattr";

const VERSION_PREFIX: &str = "version: ";
const BUILD_PREFIX: &str = "build ";
const COMMIT_SEPARATOR: &str = ", commit ";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeVersion {
    pub build: u32,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// `None` cuando el proceso termina por una señal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(
        &self,
        program: &Path,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<CommandOutput, RuntimeError>;
}

pub struct TokioCommandRunner;

#[async_trait]
impl CommandRunner for TokioCommandRunner {
    async fn run(
        &self,
        program: &Path,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<CommandOutput, RuntimeError> {
        let mut command = tokio::process::Command::new(program);
        command.args(args).stdin(Stdio::null()).kill_on_drop(true);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        hide_console_async(&mut command);
        let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
            .await
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "el proceso tardó demasiado")
            })??;
        Ok(CommandOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Busca la línea de versión en `stdout` y, si no, en `stderr`. llama.cpp la imprime
/// por `stderr`, detrás de las líneas de inicio de CUDA. Dos formatos:
/// `version: <build> (<commit>)` y `version: <semver> (build <build>, commit <commit>)`.
pub fn parse_version(stdout: &str, stderr: &str) -> Option<RuntimeVersion> {
    stdout
        .lines()
        .chain(stderr.lines())
        .find_map(|line| parse_version_line(line.trim()))
}

fn parse_version_line(line: &str) -> Option<RuntimeVersion> {
    let (head, rest) = line.strip_prefix(VERSION_PREFIX)?.split_once(" (")?;
    let details = rest.strip_suffix(')')?;
    let (build, commit) = match details.strip_prefix(BUILD_PREFIX) {
        Some(labeled) => labeled.split_once(COMMIT_SEPARATOR)?,
        None => (head, details),
    };
    if head.contains(' ') || !is_lower_hex(commit) {
        return None;
    }
    Some(RuntimeVersion {
        build: build.parse().ok()?,
        commit: commit.to_string(),
    })
}

fn is_lower_hex(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Ejecuta `server --version` desde su propio directorio, donde se resuelven los DLL.
pub async fn verify_server(
    runner: &dyn CommandRunner,
    server: &Path,
) -> Result<RuntimeVersion, RuntimeError> {
    let output = runner
        .run(server, &["--version"], server.parent())
        .await
        .map_err(|error| verification_failed(&error_chain(&error)))?;
    if !output.success() {
        return Err(verification_failed(&output.stderr));
    }
    parse_version(&output.stdout, &output.stderr).ok_or_else(|| verification_failed(&output.stderr))
}

fn verification_failed(stderr: &str) -> RuntimeError {
    RuntimeError::VerificationFailed {
        stderr_tail: tail_chars(stderr),
    }
}

/// El error y su causa: `RuntimeError::Io` no repite la causa en su mensaje.
fn error_chain(error: &RuntimeError) -> String {
    match error.source() {
        Some(source) => format!("{error}: {source}"),
        None => error.to_string(),
    }
}

fn tail_chars(text: &str) -> String {
    let skip = text.chars().count().saturating_sub(STDERR_TAIL_CHARS);
    text.chars().skip(skip).collect()
}

/// Quita el atributo de cuarentena de macOS (Gatekeeper) de todo el directorio.
/// En Windows no hace nada.
pub async fn remove_quarantine(
    runner: &dyn CommandRunner,
    os: Os,
    dir: &Path,
) -> Result<(), RuntimeError> {
    if os != Os::MacOs {
        return Ok(());
    }
    let dir = dir
        .to_str()
        .ok_or_else(|| RuntimeError::QuarantineFailed("la ruta no es UTF-8".to_string()))?;
    let output = runner
        .run(Path::new(XATTR), &["-cr", dir], None)
        .await
        .map_err(|error| RuntimeError::QuarantineFailed(error_chain(&error)))?;
    if output.success() {
        Ok(())
    } else {
        Err(RuntimeError::QuarantineFailed(tail_chars(&output.stderr)))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::runtime::test_support::FakeRunner;
    use crate::runtime::{Os, RuntimeError};

    const VERSION_CPU: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/version_cpu.txt"
    ));
    const VERSION_CUDA_STDERR: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/version_cuda_stderr.txt"
    ));

    const VERSION_SEMVER: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/version_semver.txt"
    ));

    fn version(build: u32, commit: &str) -> RuntimeVersion {
        RuntimeVersion {
            build,
            commit: commit.to_string(),
        }
    }

    #[test]
    fn version_en_stderr() {
        assert_eq!(
            parse_version("", VERSION_CPU),
            Some(version(12780, "a1b2c3d"))
        );
    }

    #[test]
    fn version_tras_lineas_cuda() {
        assert_eq!(
            parse_version("", VERSION_CUDA_STDERR),
            Some(version(12780, "a1b2c3d"))
        );
    }

    #[test]
    fn version_en_stdout() {
        assert_eq!(
            parse_version(VERSION_CPU, ""),
            Some(version(12780, "a1b2c3d"))
        );
    }

    #[test]
    fn version_con_crlf() {
        assert_eq!(
            parse_version("version: 5 (abc)\r\n", ""),
            Some(version(5, "abc"))
        );
    }

    #[test]
    fn version_formato_semver() {
        assert_eq!(
            parse_version("", VERSION_SEMVER),
            Some(version(12816, "7b74cb92d"))
        );
    }

    #[test]
    fn version_formato_semver_tras_lineas_cuda() {
        let stderr = format!(
            "ggml_cuda_init: found 1 CUDA devices:\r\n  Device 0: RTX 4070\r\n{VERSION_SEMVER}"
        );
        assert_eq!(
            parse_version("", &stderr),
            Some(version(12816, "7b74cb92d"))
        );
    }

    #[test]
    fn version_no_parseable() {
        for text in [
            "",
            "version: x (abc)",
            "version 12 (abc)",
            "version: 12 abc",
            "version: 1.1.0-dev",
            "version: 1.1.0-dev (build x, commit abc)",
            "version: 1.1.0-dev (build 12816)",
            "version: 1.1.0-dev (commit 7b74cb92d)",
            "version: 1.1.0-dev (build 12816, commit XYZ)",
            "version: (build 12816, commit abc)",
        ] {
            assert_eq!(parse_version(text, ""), None, "{text:?}");
            assert_eq!(parse_version("", text), None, "{text:?}");
        }
    }

    fn server() -> PathBuf {
        PathBuf::from("runtime").join("b1").join("llama-server")
    }

    #[tokio::test]
    async fn verify_ok_usa_cwd_del_binario() {
        let runner = FakeRunner::exits(0, "", VERSION_CPU);
        let server = server();

        let result = verify_server(&runner, &server).await.unwrap();

        assert_eq!(result, version(12780, "a1b2c3d"));
        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].program, server);
        assert_eq!(calls[0].args, ["--version"]);
        assert_eq!(calls[0].cwd.as_deref(), server.parent());
    }

    #[tokio::test]
    async fn verify_codigo_no_cero() {
        let runner = FakeRunner::exits(1, "", "missing cudart64_12.dll");

        let result = verify_server(&runner, &server()).await;

        match result {
            Err(RuntimeError::VerificationFailed { stderr_tail }) => {
                assert!(stderr_tail.contains("missing cudart64_12.dll"));
            }
            other => panic!("resultado inesperado: {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_trunca_stderr_a_2000_caracteres() {
        let noise = "é".repeat(5000);
        let runner = FakeRunner::exits(1, "", &format!("{noise}FIN"));

        let result = verify_server(&runner, &server()).await;

        match result {
            Err(RuntimeError::VerificationFailed { stderr_tail }) => {
                assert_eq!(stderr_tail.chars().count(), 2000);
                assert!(stderr_tail.ends_with("FIN"));
            }
            other => panic!("resultado inesperado: {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_sin_version() {
        let runner = FakeRunner::exits(0, "hola", "");

        let result = verify_server(&runner, &server()).await;

        assert!(matches!(
            result,
            Err(RuntimeError::VerificationFailed { .. })
        ));
    }

    #[tokio::test]
    async fn verify_error_al_lanzar() {
        let runner = FakeRunner::fails_to_launch();

        let result = verify_server(&runner, &server()).await;

        assert!(matches!(
            result,
            Err(RuntimeError::VerificationFailed { .. })
        ));
    }

    #[tokio::test]
    async fn quarantine_macos_llama_xattr() {
        let runner = FakeRunner::exits(0, "", "");
        let dir = Path::new("runtime").join("b1");

        remove_quarantine(&runner, Os::MacOs, &dir).await.unwrap();

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].program, Path::new("/usr/bin/xattr"));
        assert_eq!(calls[0].args, ["-cr", dir.to_str().unwrap()]);
    }

    #[tokio::test]
    async fn quarantine_windows_no_hace_nada() {
        let runner = FakeRunner::exits(0, "", "");

        remove_quarantine(&runner, Os::Windows, Path::new("runtime"))
            .await
            .unwrap();

        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn quarantine_fallo() {
        let runner = FakeRunner::exits(1, "", "denied");

        let result = remove_quarantine(&runner, Os::MacOs, Path::new("runtime")).await;

        assert!(matches!(result, Err(RuntimeError::QuarantineFailed(_))));
    }

    #[tokio::test]
    async fn runner_real_programa_inexistente() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-existe");

        let result = TokioCommandRunner.run(&missing, &["--version"], None).await;

        assert!(matches!(result, Err(RuntimeError::Io(_))));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn quarantine_real_macos_limpia_atributo() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("llama-server");
        std::fs::write(&file, b"x").unwrap();
        let xattr = Path::new("/usr/bin/xattr");
        let file_arg = file.to_str().unwrap();
        let marked = TokioCommandRunner
            .run(
                xattr,
                &[
                    "-w",
                    "com.apple.quarantine",
                    "0081;00000000;app-llama;",
                    file_arg,
                ],
                None,
            )
            .await
            .unwrap();
        assert!(marked.success());

        remove_quarantine(&TokioCommandRunner, Os::MacOs, dir.path())
            .await
            .unwrap();

        let read = TokioCommandRunner
            .run(xattr, &["-p", "com.apple.quarantine", file_arg], None)
            .await
            .unwrap();
        assert!(!read.success());
    }
}

//! Dobles de test compartidos por los módulos de `runtime`.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use crate::runtime::{
    Arch, ArtifactFetcher, CancelToken, CommandOutput, CommandRunner, DirRemover, FetchRequest, Os,
    Release, ReleaseSource, RuntimeError, SystemProbe,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCall {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

enum Reply {
    Output(CommandOutput),
    LaunchError,
}

/// `CommandRunner` que no lanza procesos: registra las llamadas y responde siempre igual.
pub struct FakeRunner {
    reply: Reply,
    calls: Mutex<Vec<RecordedCall>>,
}

impl FakeRunner {
    fn with_reply(reply: Reply) -> Self {
        Self {
            reply,
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn exits(code: i32, stdout: &str, stderr: &str) -> Self {
        Self::with_reply(Reply::Output(CommandOutput {
            exit_code: Some(code),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }))
    }

    pub fn fails_to_launch() -> Self {
        Self::with_reply(Reply::LaunchError)
    }

    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl CommandRunner for FakeRunner {
    async fn run(
        &self,
        program: &Path,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<CommandOutput, RuntimeError> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(RecordedCall {
                program: program.to_path_buf(),
                args: args.iter().map(ToString::to_string).collect(),
                cwd: cwd.map(Path::to_path_buf),
            });
        match &self.reply {
            Reply::Output(output) => Ok(output.clone()),
            Reply::LaunchError => Err(RuntimeError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no se pudo lanzar",
            ))),
        }
    }
}

pub struct FakeProbe {
    os: Os,
    arch: Arch,
    cpu_features: HashSet<&'static str>,
    outputs: HashMap<String, String>,
    existing_files: HashSet<PathBuf>,
    system_root: Option<PathBuf>,
    runs: Mutex<Vec<String>>,
}

impl FakeProbe {
    pub fn new(os: Os, arch: Arch) -> Self {
        Self {
            os,
            arch,
            cpu_features: HashSet::new(),
            outputs: HashMap::new(),
            existing_files: HashSet::new(),
            system_root: None,
            runs: Mutex::new(Vec::new()),
        }
    }

    pub fn with_cpu(mut self, features: &[&'static str]) -> Self {
        self.cpu_features.extend(features);
        self
    }

    pub fn with_output(mut self, program: &str, args: &[&str], stdout: &str) -> Self {
        self.outputs
            .insert(command_line(program, args), stdout.to_string());
        self
    }

    pub fn with_file(mut self, path: PathBuf) -> Self {
        self.existing_files.insert(path);
        self
    }

    pub fn with_system_root(mut self, root: PathBuf) -> Self {
        self.system_root = Some(root);
        self
    }

    pub fn run_count(&self) -> usize {
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

fn command_line(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}

impl SystemProbe for FakeProbe {
    fn os(&self) -> Os {
        self.os
    }

    fn arch(&self) -> Arch {
        self.arch
    }

    fn cpu_has(&self, feature: &str) -> bool {
        self.cpu_features.contains(feature)
    }

    fn run(&self, program: &str, args: &[&str]) -> Option<String> {
        let line = command_line(program, args);
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line.clone());
        self.outputs.get(&line).cloned()
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.existing_files.contains(path)
    }

    fn system_root(&self) -> Option<PathBuf> {
        self.system_root.clone()
    }
}

enum SourceReply {
    Releases(Vec<Release>),
    RateLimited,
}

/// `ReleaseSource` que responde siempre lo mismo, sin red.
pub struct FakeSource {
    reply: SourceReply,
}

impl FakeSource {
    pub fn with_releases(releases: Vec<Release>) -> Self {
        Self {
            reply: SourceReply::Releases(releases),
        }
    }

    pub fn rate_limited() -> Self {
        Self {
            reply: SourceReply::RateLimited,
        }
    }
}

#[async_trait::async_trait]
impl ReleaseSource for FakeSource {
    async fn list_releases(&self) -> Result<Vec<Release>, RuntimeError> {
        match &self.reply {
            SourceReply::Releases(releases) => Ok(releases.clone()),
            SourceReply::RateLimited => Err(RuntimeError::RateLimited { reset_epoch: None }),
        }
    }
}

/// `ArtifactFetcher` que no descarga nada: falla si alguien lo usa.
pub struct FakeFetcher;

#[async_trait::async_trait]
impl ArtifactFetcher for FakeFetcher {
    async fn fetch(
        &self,
        _req: FetchRequest,
        _progress: &(dyn Fn(u64, u64) + Send + Sync),
        _cancel: &CancelToken,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::Io(std::io::Error::other(
            "no se esperaba ninguna descarga",
        )))
    }
}

/// `DirRemover` que o bien borra de verdad o bien falla siempre.
pub struct FakeRemover {
    fails: bool,
}

impl FakeRemover {
    pub fn deleting() -> Self {
        Self { fails: false }
    }

    pub fn failing() -> Self {
        Self { fails: true }
    }
}

impl DirRemover for FakeRemover {
    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
        if self.fails {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fichero bloqueado",
            ));
        }
        std::fs::remove_dir_all(path)
    }
}

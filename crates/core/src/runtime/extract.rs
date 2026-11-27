//! Extracción segura de los archivos de llama.cpp y localización de `llama-server`.
use std::collections::HashSet;
use std::fmt::Display;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::runtime::{ArchiveFormat, CancelToken, Os, RuntimeError};

/// Profundidad máxima (en componentes de la ruta relativa) a la que se busca `llama-server`.
const MAX_SEARCH_DEPTH: usize = 4;

/// setuid, setgid y sticky: nunca se restauran desde un archivo descargado.
const SPECIAL_MODE_BITS: u32 = 0o7000;

/// Extrae `archive` en `dest` sin escribir nunca fuera de `dest`.
///
/// Rechaza las rutas absolutas, las que contienen `..`, los enlaces con destino absoluto o con `..`
/// y cualquier entrada que cuelgue de un enlace simbólico del propio archivo.
pub fn extract_archive(
    archive: &Path,
    format: ArchiveFormat,
    dest: &Path,
    cancel: &CancelToken,
) -> Result<(), RuntimeError> {
    fs::create_dir_all(dest)?;
    let file = fs::File::open(archive)?;
    match format {
        ArchiveFormat::Zip => extract_zip(file, dest, cancel),
        ArchiveFormat::TarGz => extract_targz(file, dest, cancel),
    }
}

fn entry_name(entry: &zip::read::ZipFile<'_, fs::File>) -> String {
    String::from_utf8_lossy(entry.name_raw()).into_owned()
}

fn extract_error(error: impl Display) -> RuntimeError {
    RuntimeError::Extract(error.to_string())
}

fn extract_zip(file: fs::File, dest: &Path, cancel: &CancelToken) -> Result<(), RuntimeError> {
    let mut archive = zip::ZipArchive::new(file).map_err(extract_error)?;
    for index in 0..archive.len() {
        cancel.check()?;
        let mut entry = archive.by_index(index).map_err(extract_error)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| RuntimeError::UnsafeArchivePath(entry_name(&entry)))?;
        let target = dest.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(&mut entry, &mut fs::File::create(&target)?)?;
        restore_unix_mode(&target, entry.unix_mode())?;
    }
    Ok(())
}

#[cfg(unix)]
fn restore_unix_mode(path: &Path, mode: Option<u32>) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match mode {
        Some(mode) => fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777)),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn restore_unix_mode(_path: &Path, _mode: Option<u32>) -> io::Result<()> {
    Ok(())
}

fn extract_targz(file: fs::File, dest: &Path, cancel: &CancelToken) -> Result<(), RuntimeError> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    archive.set_preserve_permissions(true);
    archive.set_mask(SPECIAL_MODE_BITS);
    let mut symlinks = HashSet::new();
    for entry in archive.entries().map_err(extract_error)? {
        cancel.check()?;
        let mut entry = entry.map_err(extract_error)?;
        ensure_entry_is_safe(&mut entry, &mut symlinks)?;
        let name = entry.path().map_err(extract_error)?.display().to_string();
        // `unpack_in` devuelve `false` cuando descarta la entrada por salir de `dest`.
        if !entry.unpack_in(dest).map_err(extract_error)? {
            return Err(RuntimeError::UnsafeArchivePath(name));
        }
    }
    Ok(())
}

/// Valida la entrada y anota las rutas de enlaces simbólicos ya vistas en `symlinks`.
///
/// Los enlaces solo pueden apuntar a un destino sin `..` ni raíz, y ninguna entrada puede
/// colgar de un enlace simbólico del propio archivo: así nada resuelve fuera de `dest`.
fn ensure_entry_is_safe<R: io::Read>(
    entry: &mut tar::Entry<R>,
    symlinks: &mut HashSet<PathBuf>,
) -> Result<(), RuntimeError> {
    let path = entry.path().map_err(extract_error)?.into_owned();
    let unsafe_path = || RuntimeError::UnsafeArchivePath(path.display().to_string());
    if path.components().any(escapes_destination) {
        return Err(unsafe_path());
    }
    let normalized = normal_components(&path);
    if has_symlink_ancestor(&normalized, symlinks) {
        return Err(unsafe_path());
    }
    let entry_type = entry.header().entry_type();
    if !matches!(entry_type, tar::EntryType::Symlink | tar::EntryType::Link) {
        return Ok(());
    }
    let target = entry
        .link_name()
        .map_err(extract_error)?
        .ok_or_else(unsafe_path)?;
    if target.components().any(escapes_destination) {
        return Err(unsafe_path());
    }
    if entry_type == tar::EntryType::Symlink {
        symlinks.insert(normalized);
    }
    Ok(())
}

fn escapes_destination(component: Component) -> bool {
    matches!(
        component,
        Component::ParentDir | Component::RootDir | Component::Prefix(_)
    )
}

/// La ruta sin `.`: `./l` y `l` son la misma entrada.
fn normal_components(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .collect()
}

fn has_symlink_ancestor(path: &Path, symlinks: &HashSet<PathBuf>) -> bool {
    path.ancestors()
        .skip(1)
        .any(|ancestor| symlinks.contains(ancestor))
}

/// Busca `llama-server[.exe]` sin suponer la estructura del archivo: gana el menos profundo y,
/// con empate, el primero en orden lexicográfico.
pub fn find_server_binary(root: &Path, os: Os) -> Result<PathBuf, RuntimeError> {
    let name = match os {
        Os::Windows => "llama-server.exe",
        Os::MacOs => "llama-server",
    };
    let mut candidates = Vec::new();
    collect_named(root, name, 1, &mut candidates)?;
    candidates
        .into_iter()
        .min_by(|(depth_a, path_a), (depth_b, path_b)| {
            depth_a.cmp(depth_b).then_with(|| path_a.cmp(path_b))
        })
        .map(|(_, path)| path)
        .ok_or(RuntimeError::ServerBinaryNotFound)
}

fn collect_named(
    dir: &Path,
    name: &str,
    depth: usize,
    found: &mut Vec<(usize, PathBuf)>,
) -> Result<(), RuntimeError> {
    if depth > MAX_SEARCH_DEPTH {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_named(&path, name, depth + 1, found)?;
        } else if file_type.is_file() && entry.file_name() == name {
            found.push((depth, path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::*;

    struct Entry<'a> {
        path: &'a str,
        content: &'a [u8],
        mode: u32,
    }

    fn file<'a>(path: &'a str, content: &'a [u8]) -> Entry<'a> {
        Entry {
            path,
            content,
            mode: 0o644,
        }
    }

    fn write_zip(path: &Path, entries: &[Entry]) {
        let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for entry in entries {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .unix_permissions(entry.mode);
            writer.start_file(entry.path, options).unwrap();
            writer.write_all(entry.content).unwrap();
        }
        writer.finish().unwrap();
    }

    fn targz_builder(path: &Path) -> tar::Builder<flate2::write::GzEncoder<fs::File>> {
        let encoder = flate2::write::GzEncoder::new(
            fs::File::create(path).unwrap(),
            flate2::Compression::fast(),
        );
        tar::Builder::new(encoder)
    }

    /// Escribe el nombre directamente en la cabecera: `Builder::set_path` rechaza `..`.
    fn write_targz(path: &Path, entries: &[Entry]) {
        let mut builder = targz_builder(path);
        for entry in entries {
            let mut header = tar::Header::new_gnu();
            header.as_old_mut().name[..entry.path.len()].copy_from_slice(entry.path.as_bytes());
            header.set_size(entry.content.len() as u64);
            header.set_mode(entry.mode);
            header.set_cksum();
            builder.append(&header, entry.content).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    /// Escribe el nombre en bruto en la cabecera: `Builder::append_link` normaliza `./`.
    fn write_targz_symlinks(path: &Path, links: &[(&str, &str)]) {
        let mut builder = targz_builder(path);
        for (name, target) in links {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_link_name(target).unwrap();
            header.set_cksum();
            builder.append(&header, std::io::empty()).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    fn extract(archive: &Path, format: ArchiveFormat, dest: &Path) -> Result<(), RuntimeError> {
        extract_archive(archive, format, dest, &CancelToken::default())
    }

    #[test]
    fn zip_plano_se_extrae() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        let dest = dir.path().join("out");
        write_zip(
            &archive,
            &[file("llama-server.exe", b"exe"), file("ggml.dll", b"dll")],
        );

        extract(&archive, ArchiveFormat::Zip, &dest).unwrap();

        assert_eq!(fs::read(dest.join("llama-server.exe")).unwrap(), b"exe");
        assert_eq!(fs::read(dest.join("ggml.dll")).unwrap(), b"dll");
    }

    #[test]
    fn targz_con_carpeta_se_extrae() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        let dest = dir.path().join("out");
        write_targz(
            &archive,
            &[
                file("llama-b1/llama-server", b"bin"),
                file("llama-b1/libllama.dylib", b"lib"),
            ],
        );

        extract(&archive, ArchiveFormat::TarGz, &dest).unwrap();

        assert_eq!(
            fs::read(dest.join("llama-b1/llama-server")).unwrap(),
            b"bin"
        );
        assert_eq!(
            fs::read(dest.join("llama-b1/libllama.dylib")).unwrap(),
            b"lib"
        );
    }

    #[cfg(unix)]
    #[test]
    fn targz_preserva_ejecutable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        let dest = dir.path().join("out");
        write_targz(
            &archive,
            &[Entry {
                path: "llama-server",
                content: b"bin",
                mode: 0o755,
            }],
        );

        extract(&archive, ArchiveFormat::TarGz, &dest).unwrap();

        let mode = fs::metadata(dest.join("llama-server"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
    }

    #[test]
    fn zip_slip_rechazado() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        let dest = dir.path().join("out");
        write_zip(&archive, &[file("../evil.txt", b"x")]);

        let result = extract(&archive, ArchiveFormat::Zip, &dest);

        assert!(matches!(result, Err(RuntimeError::UnsafeArchivePath(_))));
        assert!(!dir.path().join("evil.txt").exists());
    }

    #[test]
    fn targz_ruta_absoluta_rechazada() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        let dest = dir.path().join("out");
        write_targz(&archive, &[file("/tmp/evil", b"x")]);

        let result = extract(&archive, ArchiveFormat::TarGz, &dest);

        assert!(matches!(result, Err(RuntimeError::UnsafeArchivePath(_))));
        assert!(!dest.join("tmp").exists());
    }

    #[test]
    fn targz_ruta_con_punto_punto_rechazada() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        let dest = dir.path().join("out");
        write_targz(&archive, &[file("../evil.txt", b"x")]);

        let result = extract(&archive, ArchiveFormat::TarGz, &dest);

        assert!(matches!(result, Err(RuntimeError::UnsafeArchivePath(_))));
        assert!(!dir.path().join("evil.txt").exists());
    }

    // Se rechaza antes de desempaquetar, así que no depende de la plataforma.
    #[test]
    fn targz_symlink_fuera_rechazado() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        let dest = dir.path().join("out");
        write_targz_symlinks(&archive, &[("lib", "../../etc")]);

        let result = extract(&archive, ArchiveFormat::TarGz, &dest);

        assert!(matches!(result, Err(RuntimeError::UnsafeArchivePath(_))));
    }

    #[test]
    fn targz_symlink_absoluto_rechazado() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        let dest = dir.path().join("out");
        write_targz_symlinks(&archive, &[("lib", "/etc")]);

        let result = extract(&archive, ArchiveFormat::TarGz, &dest);

        assert!(matches!(result, Err(RuntimeError::UnsafeArchivePath(_))));
    }

    #[test]
    fn targz_symlink_con_punto_barra_rechazado() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        let dest = dir.path().join("out");
        write_targz_symlinks(&archive, &[("./l", "..")]);

        let result = extract(&archive, ArchiveFormat::TarGz, &dest);

        assert!(matches!(result, Err(RuntimeError::UnsafeArchivePath(_))));
        assert!(fs::symlink_metadata(dest.join("l")).is_err());
    }

    #[test]
    fn targz_symlink_encadenado_rechazado() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        let dest = dir.path().join("out");
        write_targz_symlinks(&archive, &[("a/l", ".."), ("a/l/m", "..")]);

        let result = extract(&archive, ArchiveFormat::TarGz, &dest);

        assert!(matches!(result, Err(RuntimeError::UnsafeArchivePath(_))));
        assert!(fs::symlink_metadata(dest.join("m")).is_err());
    }

    // En Windows crear enlaces simbólicos exige privilegios, por eso solo corre en Unix.
    #[cfg(unix)]
    #[test]
    fn targz_symlink_en_la_misma_carpeta_se_extrae() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.tar.gz");
        let dest = dir.path().join("out");
        let mut builder = targz_builder(&archive);
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o644);
        builder
            .append_data(&mut header, "libllama.0.dylib", &b"lib"[..])
            .unwrap();
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_size(0);
        link.set_mode(0o777);
        builder
            .append_link(&mut link, "libllama.dylib", "libllama.0.dylib")
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        extract(&archive, ArchiveFormat::TarGz, &dest).unwrap();

        assert_eq!(fs::read(dest.join("libllama.dylib")).unwrap(), b"lib");
    }

    // Un enlace válido (`a/l -> sub`) seguido de una entrada que cuelga de él: solo
    // `has_symlink_ancestor` puede rechazarla, porque el destino del enlace no tiene `..`.
    #[cfg(unix)]
    #[test]
    fn targz_entrada_bajo_symlink_valido_rechazada() {
        for colgada in ["a/l/x", "./a/l/x"] {
            let dir = tempfile::tempdir().unwrap();
            let archive = dir.path().join("a.tar.gz");
            let dest = dir.path().join("out");
            let mut builder = targz_builder(&archive);
            let mut link = tar::Header::new_gnu();
            link.set_entry_type(tar::EntryType::Symlink);
            link.set_size(0);
            link.set_mode(0o777);
            builder.append_link(&mut link, "a/l", "sub").unwrap();
            let mut header = tar::Header::new_gnu();
            header.as_old_mut().name[..colgada.len()].copy_from_slice(colgada.as_bytes());
            header.set_size(1);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &b"x"[..]).unwrap();
            builder.into_inner().unwrap().finish().unwrap();

            let result = extract(&archive, ArchiveFormat::TarGz, &dest);

            assert!(matches!(result, Err(RuntimeError::UnsafeArchivePath(_))));
        }
    }

    #[test]
    fn normal_components_ignora_punto_inicial() {
        assert_eq!(
            normal_components(Path::new("./a/./x")),
            normal_components(Path::new("a/x"))
        );
        assert_eq!(normal_components(Path::new("./x")), PathBuf::from("x"));
    }

    #[test]
    fn archivo_corrupto() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.bin");
        fs::write(&archive, b"not a zip").unwrap();

        for format in [ArchiveFormat::Zip, ArchiveFormat::TarGz] {
            let result = extract(&archive, format, &dir.path().join("out"));
            assert!(matches!(result, Err(RuntimeError::Extract(_))));
        }
    }

    #[test]
    fn extraccion_cancelada() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.zip");
        let targz_path = dir.path().join("a.tar.gz");
        write_zip(&zip_path, &[file("a.txt", b"a")]);
        write_targz(&targz_path, &[file("a.txt", b"a")]);
        let cancel = CancelToken::default();
        cancel.cancel();

        for (archive, format) in [
            (zip_path, ArchiveFormat::Zip),
            (targz_path, ArchiveFormat::TarGz),
        ] {
            let dest = dir.path().join("out");
            let result = extract_archive(&archive, format, &dest, &cancel);
            assert!(matches!(result, Err(RuntimeError::Cancelled)));
            assert!(!dest.join("a.txt").exists());
        }
    }

    fn touch(root: &Path, relative: &str) -> PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"x").unwrap();
        path
    }

    #[test]
    fn busca_server_en_raiz() {
        let dir = tempfile::tempdir().unwrap();
        let expected = touch(dir.path(), "llama-server.exe");

        assert_eq!(
            find_server_binary(dir.path(), Os::Windows).unwrap(),
            expected
        );
    }

    #[test]
    fn busca_server_anidado() {
        let dir = tempfile::tempdir().unwrap();
        let expected = touch(dir.path(), "build/bin/llama-server");

        assert_eq!(find_server_binary(dir.path(), Os::MacOs).unwrap(), expected);
    }

    #[test]
    fn busca_server_el_menos_profundo() {
        let dir = tempfile::tempdir().unwrap();
        let expected = touch(dir.path(), "a/llama-server");
        touch(dir.path(), "a/b/llama-server");

        assert_eq!(find_server_binary(dir.path(), Os::MacOs).unwrap(), expected);
    }

    #[test]
    fn busca_server_empate_orden_lexicografico() {
        let dir = tempfile::tempdir().unwrap();
        let expected = touch(dir.path(), "a/llama-server");
        touch(dir.path(), "b/llama-server");

        assert_eq!(find_server_binary(dir.path(), Os::MacOs).unwrap(), expected);
    }

    #[test]
    fn busca_server_respeta_el_sistema_operativo() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "llama-server");

        let result = find_server_binary(dir.path(), Os::Windows);

        assert!(matches!(result, Err(RuntimeError::ServerBinaryNotFound)));
    }

    #[test]
    fn busca_server_limite_de_profundidad() {
        let dir = tempfile::tempdir().unwrap();
        let within = touch(dir.path(), "a/b/c/llama-server");
        assert_eq!(find_server_binary(dir.path(), Os::MacOs).unwrap(), within);

        let deep = tempfile::tempdir().unwrap();
        touch(deep.path(), "a/b/c/d/llama-server");
        let result = find_server_binary(deep.path(), Os::MacOs);

        assert!(matches!(result, Err(RuntimeError::ServerBinaryNotFound)));
    }

    #[cfg(unix)]
    #[test]
    fn busca_server_ignora_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let real = touch(dir.path(), "real/other");
        std::os::unix::fs::symlink(&real, dir.path().join("llama-server")).unwrap();

        let result = find_server_binary(dir.path(), Os::MacOs);

        assert!(matches!(result, Err(RuntimeError::ServerBinaryNotFound)));
    }

    #[test]
    fn busca_server_ausente() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "llama-cli");

        let result = find_server_binary(dir.path(), Os::MacOs);

        assert!(matches!(result, Err(RuntimeError::ServerBinaryNotFound)));
    }

    #[test]
    fn busca_server_ignora_nombre_parecido() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "llama-server-old");
        touch(dir.path(), "llama-server.exe.bak");

        for os in [Os::Windows, Os::MacOs] {
            let result = find_server_binary(dir.path(), os);
            assert!(matches!(result, Err(RuntimeError::ServerBinaryNotFound)));
        }
    }
}

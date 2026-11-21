//! Unpacking a release archive: a `.tar.gz` on Unix, a `.zip` on Windows,
//! both holding one `nsh/` directory.

use std::{
    fs,
    io::{Read as _, Seek as _},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};

/// Unpacks `archive` into `into` and returns the archive's `nsh/` directory.
pub(crate) fn unpack(archive: &Path, into: &Path) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(into).with_context(|| format!("create {}", into.display()))?;
    let mut file =
        fs::File::open(archive).with_context(|| format!("open {}", archive.display()))?;
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic)
        .with_context(|| format!("read {}", archive.display()))?;
    file.rewind()?;
    match &magic {
        [0x1f, 0x8b] => {
            let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
            tar.set_preserve_permissions(true);
            unpack_tar(&mut tar, into).with_context(|| format!("unpack {}", archive.display()))?;
        }
        b"PK" => zip::ZipArchive::new(file)
            .and_then(|mut zip| zip.extract(into))
            .with_context(|| format!("unpack {}", archive.display()))?,
        _ => bail!("{} is not a .tar.gz or .zip", archive.display()),
    }
    let root = single_entry(into)?;
    let bin = root.join("bin").join(super::exec::exe("nsh"));
    if !bin.is_file() {
        bail!("{} holds no nsh/bin/nsh", archive.display());
    }
    executable(&bin)?;
    Ok(root)
}

/// As `tar::Archive::unpack`, without the AppleDouble files (`._<name>`)
/// macOS's `tar` adds for extended attributes.
fn unpack_tar<R: std::io::Read>(tar: &mut tar::Archive<R>, into: &Path) -> std::io::Result<()> {
    let mut directories = Vec::new();
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("._"))
        {
            continue;
        }
        if entry.header().entry_type().is_dir() {
            directories.push(entry);
        } else {
            entry.unpack_in(into)?;
        }
    }
    // Last and deepest first: a directory's mode may not let files in.
    directories.sort_by(|a, b| b.path_bytes().cmp(&a.path_bytes()));
    for mut directory in directories {
        directory.unpack_in(into)?;
    }
    Ok(())
}

/// The one directory an archive holds (`nsh/`).
fn single_entry(dir: &Path) -> anyhow::Result<PathBuf> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    match (entries.pop(), entries.is_empty()) {
        (Some(entry), true) if entry.path().is_dir() => Ok(entry.path()),
        _ => bail!("the archive must hold one directory, nsh/"),
    }
}

/// The CLI must be runnable (`zip` archives do not always carry the mode).
fn executable(bin: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = fs::metadata(bin)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(bin, permissions)?;
    }
    #[cfg(not(unix))]
    let _ = bin;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nshx-archive-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_tarball_unpacks_to_its_nsh_directory() {
        let dir = dir("tar");
        fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("nsh.tar.gz");
        {
            let file = fs::File::create(&archive).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(encoder);
            let data = b"fake";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, "nsh/bin/nsh", &data[..])
                .unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let into = dir.join("out");
        let root = unpack(&archive, &into).unwrap();
        assert_eq!(root, into.join("nsh"));
        assert_eq!(fs::read(root.join("bin/nsh")).unwrap(), b"fake");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_zip_unpacks_to_its_nsh_directory() {
        let dir = dir("zip");
        fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("nsh.zip");
        {
            let file = fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("nsh/bin/nsh", options).unwrap();
            zip.write_all(b"fake").unwrap();
            zip.finish().unwrap();
        }
        let into = dir.join("out");
        let root = unpack(&archive, &into).unwrap();
        assert_eq!(root, into.join("nsh"));
        assert_eq!(fs::read(root.join("bin/nsh")).unwrap(), b"fake");
        let _ = fs::remove_dir_all(&dir);
    }
}

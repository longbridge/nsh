//! The toolchain cache: one directory per pinned version and machine,
//! downloaded once, verified, unpacked atomically and locked while it is
//! being written.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};

use crate::{
    archive, exec, manifest,
    release::{self, RELEASE_INFO},
};

/// `NSH_CACHE`, else the platform's cache directory, plus `nsh`.
pub(crate) fn cache_root() -> anyhow::Result<PathBuf> {
    if let Some(dir) = env::var_os("NSH_CACHE").filter(|dir| !dir.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let home = env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home.map(|home| home.join("Library/Caches"))
    } else {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|home| home.join(".cache")))
    };
    Ok(base
        .context("no cache directory: set NSH_CACHE")?
        .join("nsh"))
}

/// The installed CLI of `pin` for `host`.
pub(crate) fn toolchain_dir(pin: &manifest::Pin, host: &str) -> anyhow::Result<PathBuf> {
    Ok(toolchain_dir_in(&cache_root()?, pin, host))
}

fn toolchain_dir_in(root: &Path, pin: &manifest::Pin, host: &str) -> PathBuf {
    root.join("toolchains").join(&pin.version).join(host)
}

/// Installs (once) and returns the `nsh` of `pin` for this machine.
pub(crate) fn ensure(
    app: &Path,
    config: &manifest::Config,
    pin: &manifest::Pin,
) -> anyhow::Result<PathBuf> {
    ensure_in(&cache_root()?, app, config, pin, release::BASE)
}

fn ensure_in(
    root: &Path,
    app: &Path,
    config: &manifest::Config,
    pin: &manifest::Pin,
    base: &str,
) -> anyhow::Result<PathBuf> {
    let host = release::host()?;
    let dir = toolchain_dir_in(root, pin, host);
    let bin = dir.join("bin").join(exec::exe("nsh"));
    if bin.is_file() && dir.join(".complete").is_file() {
        return Ok(bin);
    }

    let version_dir = dir.parent().context("toolchain directory")?;
    fs::create_dir_all(version_dir).with_context(|| format!("create {}", version_dir.display()))?;
    let lock_path = version_dir.join(format!(".{host}.lock"));
    let lock = fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("open {}", lock_path.display()))?;
    lock.lock()
        .with_context(|| format!("lock {}", lock_path.display()))?;
    if bin.is_file() && dir.join(".complete").is_file() {
        return Ok(bin);
    }

    let expected = manifest::expected_rev(app, config)?;
    let info = release::fetch_at(base, &pin.repository, &pin.tag)?;
    if let Some(info) = &info {
        if let Some(expected) = expected.as_deref() {
            let commit = info
                .commit
                .as_deref()
                .with_context(|| format!("{} has no commit in its nsh-release.json", pin.tag))?;
            if commit != expected {
                bail!(
                    "{} is built from {commit}, but the app pins nsh-core at {expected}",
                    pin.tag
                );
            }
        }
    } else if expected.is_some() {
        eprintln!(
            "nsh-launcher: {} has no {RELEASE_INFO}; the pinned nsh-core commit is not verified",
            pin.tag
        );
    }

    let asset = release::asset(host);
    let part = version_dir.join(format!(".{host}.part"));
    let staging = version_dir.join(format!(".{host}.tmp"));
    let _ = fs::remove_dir_all(&staging);
    let result = (|| {
        release::download(base, &pin.repository, &pin.tag, &asset, &part)?;
        let unpacked = archive::unpack(&part, &staging)?;
        if dir.exists() {
            fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
        }
        fs::rename(&unpacked, &dir).with_context(|| format!("install {}", dir.display()))?;
        fs::write(dir.join(".complete"), b"")
            .with_context(|| format!("write {}", dir.join(".complete").display()))?;
        anyhow::Ok(())
    })();
    let _ = fs::remove_file(&part);
    let _ = fs::remove_dir_all(&staging);
    result.with_context(|| {
        format!(
            "install nsh {} ({}) for {host}",
            pin.version, pin.repository
        )
    })?;
    Ok(bin)
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::{
        collections::HashMap,
        io::{Read as _, Write as _},
        net::TcpListener,
        sync::Arc,
        thread,
    };

    #[cfg(unix)]
    use sha2::{Digest as _, Sha256};

    use super::*;

    #[test]
    fn a_toolchain_lives_under_its_version_and_host() {
        let pin = manifest::Pin {
            version: "0.6.1".into(),
            tag: "v0.6.1".into(),
            repository: release::DEFAULT_REPOSITORY.into(),
        };
        assert_eq!(
            toolchain_dir_in(Path::new("/cache"), &pin, "macos-arm64"),
            Path::new("/cache/toolchains/0.6.1/macos-arm64")
        );
    }

    #[cfg(unix)]
    #[test]
    fn ensure_downloads_verifies_and_unpacks_a_release() {
        let dir = std::env::temp_dir().join(format!(
            "nsh-launcher-cache-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("app/native")).unwrap();
        let commit = "abcdef0123456789abcdef0123456789abcdef01";
        fs::write(
            dir.join("app/nsh.json"),
            r#"{"toolchain":{"nsh":{"version":"0.6.1"}},"rust":"native"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("app/native/Cargo.lock"),
            format!(
                "version = 4\n[[package]]\nname = \"nsh-core\"\nversion = \"0.6.0\"\nsource = \"git+https://github.com/longbridge/nsh?rev={commit}#{commit}\"\n"
            ),
        )
        .unwrap();

        let host = release::host().unwrap();
        let asset = release::asset(host);
        let archive = tarball();
        let hash = format!("{:x}", Sha256::digest(&archive));
        let base_path = "/longbridge/nsh/releases/download/v0.6.1";
        let files: HashMap<String, Vec<u8>> = [
            (format!("{base_path}/{asset}"), archive),
            (
                format!("{base_path}/{asset}.sha256"),
                format!("{hash}  {asset}\n").into_bytes(),
            ),
            (
                format!("{base_path}/{RELEASE_INFO}"),
                format!(r#"{{"version":"0.6.0","commit":"{commit}","tag":"v0.6.1"}}"#).into_bytes(),
            ),
        ]
        .into_iter()
        .collect();
        let (base, _server) = serve(files);

        let app = dir.join("app");
        let config = manifest::read_config(&app).unwrap();
        let pin = manifest::pin(&config).unwrap().unwrap();
        let root = dir.join("cache");
        let bin = ensure_in(&root, &app, &config, &pin, &base).unwrap();
        assert_eq!(
            bin,
            root.join("toolchains/0.6.1").join(host).join("bin/nsh")
        );
        assert_eq!(fs::read(&bin).unwrap(), b"fake");
        assert!(bin.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    fn tarball() -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        let data = b"fake";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "nsh/bin/nsh", &data[..])
            .unwrap();
        let tar = tar.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar).unwrap();
        encoder.finish().unwrap()
    }

    /// A local release host: `GET <path>` answers from `files`.
    #[cfg(unix)]
    fn serve(files: HashMap<String, Vec<u8>>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let files = Arc::new(files);
        let handle = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let files = Arc::clone(&files);
                let mut buffer = [0u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else {
                    continue;
                };
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let response = match files.get(&path) {
                    Some(body) => {
                        let mut response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        response.extend_from_slice(body);
                        response
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = stream.write_all(&response);
            }
        });
        (format!("http://{addr}"), handle)
    }
}

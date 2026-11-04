//! `nsh/update`: the app updates itself (`docs/dev/update.md`). The rules
//! are `nsh-update`'s; this module is its JS face over the platform's HTTP
//! stack ([`UpdateHost`]). The source comes from the package, never from
//! the caller, so what the app's code can do is ask, not point it elsewhere.
//!
//! Needs no capability: the developer named the source in `nsh.json`, and a
//! module running inside another app has no installation to replace.

#[cfg(not(target_arch = "wasm32"))]
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};

use crate::element::{Callback, JsError, JsObject, Later, System, codes, module};

/// Takes a piece of a body and the total size if known; an error stops it.
pub type UpdateSink<'a> = &'a mut dyn FnMut(&[u8], Option<u64>) -> Result<(), String>;

/// What the platform gives the module: the package's update configuration
/// and HTTP. Both block, and are called off the engine thread.
pub trait UpdateHost: Send + Sync + 'static {
    /// The package's configuration file; `None` when it has none (a
    /// development run, an app without `"update"` in `nsh.json`).
    fn config(&self) -> Option<Vec<u8>>;

    /// Gets `url`, following redirects, handing each piece of the body and
    /// the total size if known to `sink`, until it fails or the body ends.
    fn get(&self, url: &str, sink: UpdateSink<'_>) -> Result<(), String>;
}

/// Bytes downloaded, as `downloadUpdate`'s callback gets them.
#[derive(Debug, Clone, Copy, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub(crate) struct UpdateProgress {
    /// Bytes received so far.
    pub(crate) received: f64,
    /// All of them, when known.
    pub(crate) total: Option<f64>,
}

/// A newer version of the app.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub(crate) struct AppUpdate {
    /// Its version.
    pub(crate) version: String,
    /// What changed, as the manifest says it.
    pub(crate) notes: Option<String>,
    /// Where the user can get it by hand: the archive, or a store's page on
    /// iOS and Android. For `openUrl` when `canInstall` is false.
    pub(crate) url: String,
    /// Whether `downloadUpdate` and `installAndRestart` work here: a package
    /// on the desktop, installed where the user can write. False on iOS and
    /// Android, and under a directory only the system writes.
    pub(crate) can_install: bool,
}

/// Checking for a newer version of the app and installing it.
#[module]
pub(crate) struct Update;

#[module(flat)]
impl Update {
    /// Asks the source in `nsh.json`'s `"update"` for a newer version.
    /// Resolves with the update, or `null` when the app is up to date, has
    /// no `"update"`, or runs where apps do not update (the web, a
    /// development run). Rejects when the source cannot be reached or its
    /// manifest is not acceptable: a bad signature, another app's id, a
    /// URL that is not HTTPS.
    pub(crate) fn check_for_update(sys: System) -> Later<Option<AppUpdate>> {
        Later::new(native::check(sys))
    }

    /// Downloads the update the last `checkForUpdate` found and checks it
    /// against the manifest, calling `onProgress` as bytes arrive. Rejects
    /// with `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` when it found none or
    /// `canInstall` is false, and when the download fails or does not match.
    pub(crate) fn download_update(
        #[args(progress)] on_progress: Option<Callback<UpdateProgress>>,
        sys: System,
    ) -> Later<()> {
        Later::new(native::download(sys, on_progress))
    }

    /// Replaces the installed app with the downloaded update and starts it;
    /// this run then exits, so the promise settles only on failure. Rejects
    /// when nothing was downloaded, or when the app cannot be replaced,
    /// which leaves it as it was.
    pub(crate) fn install_and_restart(sys: System) -> Later<()> {
        Later::new(native::install(sys))
    }
}

/// Why nothing can be downloaded or installed.
fn unavailable() -> JsError {
    JsError::Error(
        "no update this run can install: check `canInstall` of `checkForUpdate`'s update, or \
         send the user to its `url`"
            .into(),
    )
    .with_code(codes::UNAVAILABLE)
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use nsh_update::{Available, Config, Download};

    use super::*;

    /// What the VM's last check found and its download left.
    #[derive(Default)]
    struct Found(RefCell<Option<Rc<Pending>>>);

    impl crate::element::Local for Found {}

    struct Pending {
        host: Arc<dyn UpdateHost>,
        config: Config,
        available: Available,
        dir: PathBuf,
        archive: RefCell<Option<PathBuf>>,
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        installation: nsh_update::install::Installation,
    }

    /// Runs `work` off the engine thread.
    async fn blocking<T: Send + 'static>(
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, JsError> {
        super::super::fs::blocking(move || work().map_err(JsError::Error)).await
    }

    pub(super) async fn check(sys: System) -> Result<Option<AppUpdate>, JsError> {
        let found = sys.locals.get::<Found>();
        *found.0.borrow_mut() = None;
        let Some(host) = sys.update.clone() else {
            return Ok(None);
        };
        let worker = host.clone();
        let checked = blocking(move || {
            let Some(bytes) = worker.config() else {
                return Ok(None);
            };
            let config = Config::parse(&bytes).map_err(|e| e.to_string())?;
            let manifest = get_all(&*worker, &config.manifest_url())?;
            let signature = match config.signature_url() {
                Some(url) => Some(get_all(&*worker, &url)?),
                None => None,
            };
            let available = config
                .check(&manifest, signature.as_deref(), &nsh_update::platform())
                .map_err(|e| e.to_string())?;
            Ok(available.map(|available| (bytes, config, available)))
        })
        .await?;
        let Some((bytes, config, available)) = checked else {
            return Ok(None);
        };
        let mut update = AppUpdate {
            version: available.version.clone(),
            notes: available.notes.clone(),
            url: available.url.clone(),
            can_install: false,
        };
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        {
            let id = config.id.clone();
            let installation = blocking(move || {
                Ok(nsh_update::install::Installation::current(&id, &bytes)
                    .inspect(|installation| installation.remove_leftovers())
                    .filter(|installation| installation.can_install()))
            })
            .await?;
            if let (Some(installation), Some(dir), true) = (
                installation,
                sys.data_dir.clone(),
                available.sha256.is_some(),
            ) {
                update.can_install = true;
                *found.0.borrow_mut() = Some(Rc::new(Pending {
                    host,
                    config,
                    available,
                    dir: dir.join("updates"),
                    archive: RefCell::new(None),
                    installation,
                }));
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        let _ = (bytes, config, host, &found);
        Ok(Some(update))
    }

    /// A small file, whole. A manifest is a few kilobytes.
    fn get_all(host: &dyn UpdateHost, url: &str) -> Result<Vec<u8>, String> {
        const MAX: usize = 1 << 20;
        let mut bytes = Vec::new();
        host.get(url, &mut |chunk, _| {
            if bytes.len() + chunk.len() > MAX {
                return Err("over 1 MiB".to_owned());
            }
            bytes.extend_from_slice(chunk);
            Ok(())
        })
        .map_err(|e| format!("{url}: {e}"))?;
        Ok(bytes)
    }

    fn pending(sys: &System) -> Result<Rc<Pending>, JsError> {
        sys.locals
            .get::<Found>()
            .0
            .borrow()
            .clone()
            .ok_or_else(unavailable)
    }

    pub(super) async fn download(
        sys: System,
        on_progress: Option<Callback<UpdateProgress>>,
    ) -> Result<(), JsError> {
        use std::io::Write as _;

        let pending = pending(&sys)?;
        let host = pending.host.clone();
        let dir = pending.dir.clone();
        let url = pending.available.url.clone();
        let sha256 = pending.available.sha256.clone().unwrap_or_default();
        let size = pending.available.size;
        let name = format!("{}-{}", pending.config.id, pending.available.version);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut work = std::pin::pin!(blocking(move || {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = dir.join(name);
            let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            let mut check = Download::new(&sha256, size);
            let result = host
                .get(&url, &mut |chunk, total| {
                    let received = check.update(chunk).map_err(|e| e.to_string())?;
                    file.write_all(chunk).map_err(|e| e.to_string())?;
                    let _ = tx.send((received, size.or(total)));
                    Ok(())
                })
                .and_then(|()| file.flush().map_err(|e| e.to_string()))
                .and_then(|()| check.finish().map_err(|e| e.to_string()));
            if let Err(e) = result {
                let _ = std::fs::remove_file(&path);
                return Err(e);
            }
            Ok(path)
        }));
        let path = loop {
            tokio::select! {
                result = &mut work => break result?,
                Some((received, total)) = rx.recv() => {
                    if let Some(callback) = &on_progress {
                        callback.post(UpdateProgress {
                            received: received as f64,
                            total: total.map(|t: u64| t as f64),
                        });
                    }
                }
            }
        };
        *pending.archive.borrow_mut() = Some(path);
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(super) async fn install(sys: System) -> Result<(), JsError> {
        let pending = pending(&sys)?;
        let archive = pending.archive.borrow().clone().ok_or_else(|| {
            JsError::Error("nothing is downloaded: call downloadUpdate first".into())
        })?;
        let installation = pending.installation.clone();
        blocking(move || {
            installation.install(&archive).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&archive);
            // The app's last storage writes are on their way to the disk, on
            // a thread of their own that this run's exit would end, and the
            // new run reads that file.
            std::thread::sleep(std::time::Duration::from_millis(300));
            installation.launch().map_err(|e| e.to_string())
        })
        .await?;
        std::process::exit(0);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    pub(super) async fn install(_: System) -> Result<(), JsError> {
        Err(unavailable())
    }
}

/// Nothing updates on the web: a deploy updates its apps.
#[cfg(target_arch = "wasm32")]
mod native {
    use super::*;

    pub(super) async fn check(_: System) -> Result<Option<AppUpdate>, JsError> {
        Ok(None)
    }

    pub(super) async fn download(
        _: System,
        _: Option<Callback<UpdateProgress>>,
    ) -> Result<(), JsError> {
        Err(unavailable())
    }

    pub(super) async fn install(_: System) -> Result<(), JsError> {
        Err(unavailable())
    }
}

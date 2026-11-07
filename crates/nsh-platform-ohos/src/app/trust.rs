//! Signed delivery uses the shared verifier. File IO, hashing, extraction and
//! persistent rollback/revocation state all run off the ArkUI thread.

use std::{
    io::Read,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex, Weak},
};

use napi_derive_ohos::napi;
use napi_ohos::{
    Env, Error, Result,
    bindgen_prelude::{Object, PromiseRaw},
};

use crate::services::assets::AssetDirectory;

const MAX_PACKAGE: u64 = 256 << 20;
type DirectoryLocks = Vec<(PathBuf, Weak<Mutex<()>>)>;
static DIRECTORIES: LazyLock<Mutex<DirectoryLocks>> = LazyLock::new(Mutex::default);

/// Verified package metadata. Build numbers are decimal strings to preserve u64 precision.
#[napi(object)]
#[derive(Clone)]
pub struct NshPackageInfo {
    /// Signed application identifier.
    pub id: String,
    /// Signed display name.
    pub name: String,
    /// Signed version.
    pub version: String,
    /// Signed build number, as decimal text.
    pub build: String,
}

pub(crate) struct Prepared {
    pub(crate) bundle: nsh_engine::Bundle,
    pub(crate) assets: AssetDirectory,
}

/// A verified package that can be consumed by one NshSurface.
#[napi]
pub struct NshPackage {
    info: NshPackageInfo,
    prepared: Mutex<Option<Prepared>>,
}

#[napi]
impl NshPackage {
    /// Signed identity and version of the verified package.
    #[napi(getter)]
    pub fn info(&self) -> NshPackageInfo {
        self.info.clone()
    }
    /// Whether a surface has consumed this package.
    #[napi(getter)]
    pub fn is_consumed(&self) -> bool {
        self.prepared.lock().unwrap().is_none()
    }
}
impl NshPackage {
    pub(crate) fn take(&self) -> Result<Prepared> {
        self.prepared
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| Error::from_reason("the package already ran"))
    }
}

/// A persistent trust store. Each method that accesses its state runs on a worker.
#[napi]
pub struct NshTrust {
    store: Arc<Mutex<nsh_package::TrustStore>>,
    assets: PathBuf,
    directory_lock: Arc<Mutex<()>>,
}

#[napi]
impl NshTrust {
    /// Opens a private state directory and installs the host's root public keys (base64).
    #[napi(factory)]
    pub async fn open(state_directory: String, roots: Vec<String>) -> Result<Self> {
        blocking(move || {
            let path = PathBuf::from(state_directory);
            std::fs::create_dir_all(&path).map_err(error)?;
            let path = path.canonicalize().map_err(error)?;
            let mut store = nsh_package::TrustStore::open(&path).map_err(error)?;
            for root in roots {
                store.add_root(nsh_package::PublicKey::from_base64(&root).map_err(error)?);
            }
            let directory_lock = {
                let mut directories = DIRECTORIES.lock().unwrap();
                directories.retain(|(_, lock)| lock.strong_count() > 0);
                if let Some(lock) = directories
                    .iter()
                    .find_map(|(dir, lock)| (dir == &path).then(|| lock.upgrade()).flatten())
                {
                    lock
                } else {
                    let lock = Arc::new(Mutex::new(()));
                    directories.push((path.clone(), Arc::downgrade(&lock)));
                    lock
                }
            };
            Ok(Self {
                store: Arc::new(Mutex::new(store)),
                assets: path.join("packages"),
                directory_lock,
            })
        })
        .await
    }

    /// Adds a root public key without waiting for an in-progress verification on the UI thread.
    #[napi]
    pub async fn add_root(&self, root: String) -> Result<()> {
        let store = self.store.clone();
        blocking(move || {
            let root = nsh_package::PublicKey::from_base64(&root).map_err(error)?;
            store.lock().unwrap().add_root(root);
            Ok(())
        })
        .await
    }

    /// Verifies and persists a newer signed revocation list. Returns its exact sequence number.
    #[napi]
    pub async fn update_revocations(&self, json: String) -> Result<String> {
        let store = self.store.clone();
        let assets = self.assets.clone();
        let directory_lock = self.directory_lock.clone();
        blocking(move || {
            let list = nsh_package::Revocations::from_json(&json).map_err(error)?;
            let _directory = directory_lock.lock().unwrap();
            let mut store = store.lock().unwrap();
            Self::refresh(&mut store, &assets)?;
            store
                .update_revocations(list)
                .map(|n| n.to_string())
                .map_err(error)
        })
        .await
    }

    /// Verifies a downloaded package, extracts assets and persists rollback protection.
    /// Rejection leaves every running surface unchanged.
    #[napi]
    pub async fn open_package(&self, path: String) -> Result<NshPackage> {
        let store = self.store.clone();
        let assets = self.assets.clone();
        let directory_lock = self.directory_lock.clone();
        blocking(move || {
            let file = std::fs::File::open(PathBuf::from(path)).map_err(error)?;
            let mut bytes = Vec::new();
            file.take(MAX_PACKAGE + 1)
                .read_to_end(&mut bytes)
                .map_err(error)?;
            if bytes.len() as u64 > MAX_PACKAGE {
                return Err(error("package exceeds 256 MiB"));
            }
            Self::prepare(&directory_lock, &store, &assets, &bytes)
        })
        .await
    }

    /// Verifies a signed HAP rawfile using the Ability resource manager bound to `uiContext`
    /// (or an active host session when `uiContext` is omitted).
    #[napi(ts_return_type = "Promise<NshPackage>")]
    pub fn open_resource<'env>(
        &self,
        env: &'env Env,
        name: String,
        #[napi(ts_arg_type = "import('@ohos.arkui.UIContext').UIContext")] ui_context: Option<
            Object<'env>,
        >,
    ) -> Result<PromiseRaw<'env, NshPackage>> {
        crate::app::session::resource_name(&name)?;
        let host = if let Some(ui_context) = ui_context {
            crate::app::entry::bind_session(env, &ui_context)?
        } else {
            crate::app::session::any_active()?
        };
        let permit = host.reserve_load()?;
        let store = self.store.clone();
        let assets = self.assets.clone();
        let directory_lock = self.directory_lock.clone();
        env.spawn_future(async move {
            blocking(move || {
                let _permit = permit;
                let bytes = host.resource(&name)?;
                Self::prepare(&directory_lock, &store, &assets, &bytes)
            })
            .await
        })
    }
}
impl NshTrust {
    fn refresh(store: &mut nsh_package::TrustStore, assets: &std::path::Path) -> Result<()> {
        // Separate store objects may share persisted rollback/revocation state,
        // but retain their own root policy. Reload under the directory lock.
        let mut fresh = nsh_package::TrustStore::open(
            assets.parent().expect("package root has a state directory"),
        )
        .map_err(error)?;
        for root in store.roots() {
            fresh.add_root(*root);
        }
        *store = fresh;
        Ok(())
    }
    fn prepare(
        directory_lock: &Mutex<()>,
        store: &Mutex<nsh_package::TrustStore>,
        assets: &std::path::Path,
        bytes: &[u8],
    ) -> Result<NshPackage> {
        let _directory = directory_lock.lock().unwrap();
        let mut store = store.lock().unwrap();
        Self::refresh(&mut store, assets)?;
        let loaded = nsh_engine::load_package(&mut store, bytes, assets).map_err(error)?;
        std::fs::create_dir_all(&loaded.assets).map_err(error)?;
        let directory = AssetDirectory::from_path(&loaded.assets).map_err(error)?;
        Ok(NshPackage {
            info: NshPackageInfo {
                id: loaded.app.id,
                name: loaded.app.name,
                version: loaded.app.version,
                build: loaded.app.build.to_string(),
            },
            prepared: Mutex::new(Some(Prepared {
                bundle: loaded.bundle,
                assets: directory,
            })),
        })
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work).await.map_err(error)?
}
fn error(value: impl std::fmt::Display) -> Error {
    Error::from_reason(value.to_string())
}

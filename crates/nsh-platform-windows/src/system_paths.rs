//! Local paths ([`nsh_core::system_paths`]): the shell's file dialogs
//! (`IFileOpenDialog`, with `FOS_PICKFOLDERS` for folders, and
//! `IFileSaveDialog`) owned by the surface's window, opening a file in its
//! app (`ShellExecuteW`), showing it selected in File Explorer
//! (`SHOpenFolderAndSelectItems`) and the Recycle Bin (`IFileOperation`
//! with `FOFX_RECYCLEONDELETE`). Each request runs on a thread of its own
//! (a single-threaded COM apartment), so the surface's message loop never
//! waits in it, and answers as a root `SystemPath` event.

use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use nsh_core::{
    EventKind, EventPayload, ROOT_ID,
    system_paths::{self, PathPickRequest, PathRequest, SavePathRequest},
};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
        UI::{
            Shell::{
                Common::COMDLG_FILTERSPEC, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI,
                FOF_SILENT, FOFX_RECYCLEONDELETE, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST,
                FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, FileOperation,
                FileSaveDialog, IFileDialog, IFileOpenDialog, IFileOperation, IFileSaveDialog,
                ILCreateFromPathW, ILFree, IShellItem, SHCreateItemFromParsingName,
                SHOpenFolderAndSelectItems, SIGDN_FILESYSPATH, ShellExecuteW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{HSTRING, PCWSTR, w},
};

use crate::surface::Surface;

/// Carries out request `id`.
pub(crate) fn request(surface: &Rc<Surface>, id: u32, request: &PathRequest) {
    let mut control = None;
    surface.control(|c| control = Some(c.clone()));
    let Some(control) = control else { return };
    // A window handle is an address the other thread only hands to the
    // dialog as its owner.
    let owner = surface.hwnd().0 as isize;
    let request = request.clone();
    let spawned = std::thread::Builder::new()
        .name("nsh-paths".into())
        .spawn(move || {
            let answer = with_com(|| answer(id, HWND(owner as *mut _), &request))
                .unwrap_or_else(|e| system_paths::failed(id, e.message()));
            control.dispatch_event(ROOT_ID, EventKind::SystemPath, answer);
        });
    if let Err(e) = spawned {
        log::warn!("cannot carry out a path request: {e}");
    }
}

/// Runs `f` in a single-threaded COM apartment of this thread.
fn with_com<T>(f: impl FnOnce() -> T) -> windows::core::Result<T> {
    // SAFETY: COM is initialized for this thread and uninitialized after
    // `f`, which released every interface it made.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let result = f();
        CoUninitialize();
        Ok(result)
    }
}

fn answer(id: u32, owner: HWND, request: &PathRequest) -> EventPayload {
    let result = match request {
        PathRequest::Pick(pick) => pick_paths(owner, pick),
        PathRequest::Save(save) => pick_save_path(owner, save).map(|p| p.into_iter().collect()),
        PathRequest::Open(path) => open(path).map(|()| Vec::new()),
        PathRequest::Reveal(path) => reveal(path).map(|()| Vec::new()),
        PathRequest::Trash(path) => trash(path).map(|()| Vec::new()),
        other => {
            log::debug!("windows: path request {other:?} is not supported");
            return system_paths::failed(id, "not supported");
        }
    };
    match result {
        Ok(paths) => system_paths::answer(id, paths),
        Err(e) => {
            if matches!(request, PathRequest::Pick(_) | PathRequest::Save(_)) {
                log::warn!("the file dialog failed: {e}");
            }
            system_paths::failed(id, e.message())
        }
    }
}

fn item(path: &Path) -> windows::core::Result<IShellItem> {
    // SAFETY: a NUL-terminated path that outlives the call.
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None) }
}

fn item_path(item: &IShellItem) -> windows::core::Result<Option<PathBuf>> {
    // SAFETY: the name the shell allocated is freed once copied.
    unsafe {
        let name = item.GetDisplayName(SIGDN_FILESYSPATH)?;
        let path = name.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(name.0 as *const _));
        Ok(path)
    }
}

/// Sets what the open and save dialogs share.
fn configure(
    dialog: &IFileDialog,
    title: Option<&str>,
    directory: Option<&PathBuf>,
    extensions: &[String],
) -> windows::core::Result<()> {
    // SAFETY: COM calls on a dialog of this thread; the strings outlive
    // the calls that read them (the dialog copies them).
    unsafe {
        if let Some(title) = title {
            dialog.SetTitle(&HSTRING::from(title))?;
        }
        if let Some(folder) = directory.and_then(|d| item(d).ok()) {
            dialog.SetFolder(&folder)?;
        }
        if !extensions.is_empty() {
            // No label of NSH's own: the filter shows its patterns.
            let pattern = extensions
                .iter()
                .map(|e| format!("*.{e}"))
                .collect::<Vec<_>>()
                .join(";");
            let pattern = HSTRING::from(pattern);
            let spec = [COMDLG_FILTERSPEC {
                pszName: PCWSTR(pattern.as_ptr()),
                pszSpec: PCWSTR(pattern.as_ptr()),
            }];
            dialog.SetFileTypes(&spec)?;
            dialog.SetDefaultExtension(&HSTRING::from(extensions[0].as_str()))?;
        }
    }
    Ok(())
}

/// The paths picked in the dialog; none when it was cancelled.
fn pick_paths(owner: HWND, request: &PathPickRequest) -> windows::core::Result<Vec<PathBuf>> {
    // SAFETY: COM calls on a dialog this thread owns until it returns.
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let mut options = dialog.GetOptions()? | FOS_FORCEFILESYSTEM;
        options |= if request.folders {
            FOS_PICKFOLDERS
        } else {
            FOS_FILEMUSTEXIST
        };
        if request.multiple {
            options |= FOS_ALLOWMULTISELECT;
        }
        dialog.SetOptions(options)?;
        let extensions: &[String] = if request.folders {
            &[]
        } else {
            &request.extensions
        };
        configure(
            &dialog,
            request.title.as_deref(),
            request.directory.as_ref(),
            extensions,
        )?;
        if dialog.Show(Some(owner)).is_err() {
            // Cancelled.
            return Ok(Vec::new());
        }
        let items = dialog.GetResults()?;
        let mut paths = Vec::new();
        for i in 0..items.GetCount()? {
            paths.extend(item_path(&items.GetItemAt(i)?)?);
        }
        Ok(paths)
    }
}

/// The path chosen in the dialog; none when it was cancelled.
fn pick_save_path(
    owner: HWND,
    request: &SavePathRequest,
) -> windows::core::Result<Option<PathBuf>> {
    // SAFETY: COM calls on a dialog this thread owns until it returns; the
    // name outlives the call that reads it.
    unsafe {
        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
        dialog.SetOptions(dialog.GetOptions()? | FOS_FORCEFILESYSTEM)?;
        if let Some(name) = &request.name {
            dialog.SetFileName(&HSTRING::from(name.as_str()))?;
        }
        configure(
            &dialog,
            request.title.as_deref(),
            request.directory.as_ref(),
            &request.extensions,
        )?;
        if dialog.Show(Some(owner)).is_err() {
            return Ok(None);
        }
        item_path(&dialog.GetResult()?)
    }
}

/// Opens `path` with its default app (its `open` verb).
fn open(path: &Path) -> windows::core::Result<()> {
    // SAFETY: plain call with valid NUL-terminated strings.
    let instance = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(path.as_os_str()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Above 32 is success; the rest are error codes.
    if instance.0 as isize > 32 {
        Ok(())
    } else {
        Err(windows::core::Error::from_thread())
    }
}

/// Shows `path` selected in File Explorer.
fn reveal(path: &Path) -> windows::core::Result<()> {
    // SAFETY: the ID list made of a NUL-terminated path is freed after the
    // call that reads it; with no items, the shell selects the item the
    // list names.
    unsafe {
        let list = ILCreateFromPathW(&HSTRING::from(path.as_os_str()));
        if list.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        let shown = SHOpenFolderAndSelectItems(list, None, 0);
        ILFree(Some(list));
        shown
    }
}

/// Moves `path` to the Recycle Bin, without asking or showing progress.
fn trash(path: &Path) -> windows::core::Result<()> {
    // SAFETY: COM calls on an operation this thread owns.
    unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
        operation.SetOperationFlags(
            FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT | FOFX_RECYCLEONDELETE,
        )?;
        operation.DeleteItem(&item(path)?, None)?;
        operation.PerformOperations()?;
        if operation.GetAnyOperationsAborted()?.as_bool() {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_ABORT,
                "moving to the Recycle Bin was cancelled",
            ));
        }
    }
    Ok(())
}

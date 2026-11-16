//! Local paths ([`nsh_core::file_dialog`]): the shell's file dialogs
//! (`IFileOpenDialog`, with `FOS_PICKFOLDERS` for folders, and
//! `IFileSaveDialog`) owned by the surface's window, opening a file in its
//! app (`ShellExecuteW`), showing it selected in File Explorer
//! (`SHOpenFolderAndSelectItems`) and the Recycle Bin (`IFileOperation`
//! with `FOFX_RECYCLEONDELETE`). A folder picker stays on the file system:
//! a folder with no path, such as Home, opens the user's profile instead.
//! Each request runs on a thread of its own
//! (a single-threaded COM apartment), so the surface's message loop never
//! waits in it, and answers as a root `FileRequest` event.

use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
};

use nsh_core::{
    EventKind, EventPayload, ROOT_ID,
    file_dialog::{FileFilter, FileRequest, OpenDialog, SaveDialog},
    picker::{FilePickRequest, read_picked_files},
};
use windows::{
    Win32::{
        Foundation::{ERROR_CANCELLED, HWND},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
        UI::{
            Shell::{
                Common::COMDLG_FILTERSPEC, FDE_OVERWRITE_RESPONSE, FDE_SHAREVIOLATION_RESPONSE,
                FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOFX_RECYCLEONDELETE,
                FOLDERID_Profile, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
                FOS_PICKFOLDERS, FileOpenDialog, FileOperation, FileSaveDialog, IFileDialog,
                IFileDialogEvents, IFileDialogEvents_Impl, IFileOpenDialog, IFileOperation,
                IFileSaveDialog, ILCreateFromPathW, ILFree, IShellItem, KF_FLAG_DEFAULT,
                SHCreateItemFromParsingName, SHGetKnownFolderPath, SHOpenFolderAndSelectItems,
                SIGDN_FILESYSPATH, ShellExecuteW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{HRESULT, HSTRING, PCWSTR, Ref, implement, w},
};

use crate::surface::Surface;

/// Carries out request `id`.
pub(crate) fn request(surface: &Rc<Surface>, id: u32, request: &FileRequest) {
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
                .unwrap_or_else(|e| EventPayload::file_failed(id, e.message()));
            control.dispatch_event(ROOT_ID, EventKind::FileRequest, answer);
        });
    if let Err(e) = spawned {
        log::warn!("cannot carry out a file request: {e}");
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

fn answer(id: u32, owner: HWND, request: &FileRequest) -> EventPayload {
    let result = match request {
        FileRequest::OpenDialog(dialog) => pick_paths(owner, dialog),
        FileRequest::SaveDialog(dialog) => {
            pick_save_path(owner, dialog).map(|p| p.into_iter().collect())
        }
        FileRequest::OpenPath(path) => open(path).map(|()| Vec::new()),
        FileRequest::RevealPath(path) => reveal(path).map(|()| Vec::new()),
        FileRequest::MoveToTrash(path) => trash(path).map(|()| Vec::new()),
        other => {
            log::debug!("windows: file request {other:?} is not supported");
            return EventPayload::file_failed(id, "not supported");
        }
    };
    match result {
        Ok(paths) => EventPayload::file_answer(id, paths),
        Err(e) => {
            if matches!(
                request,
                FileRequest::OpenDialog(_) | FileRequest::SaveDialog(_)
            ) {
                log::warn!("the file dialog failed: {e}");
            }
            EventPayload::file_failed(id, e.message())
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

/// Whether `item` has a file system path: Home and This PC do not.
fn on_file_system(item: &IShellItem) -> bool {
    matches!(item_path(item), Ok(Some(_)))
}

/// The user's profile folder.
fn profile_folder() -> Option<PathBuf> {
    // SAFETY: the shell allocates the path, which is freed once copied.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Profile, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map(PathBuf::from)
    }
}

/// Keeps a folder picker on the file system: the shell reports a folder
/// with no path (Home, This PC), and this sink opens the user's profile
/// with the name field empty, so the dialog answers the folder it shows.
#[derive(Default)]
#[implement(IFileDialogEvents)]
struct FileSystemOnly {
    /// Set while the redirect runs: `SetFolder` reports the change it
    /// makes, and a folder that stays off the file system would report it
    /// again.
    redirecting: Cell<bool>,
}

impl FileSystemOnly {
    /// Moves `dialog` from `folder`, which has no path, to the profile
    /// folder, with the name field empty.
    fn keep(&self, dialog: &IFileDialog, folder: &IShellItem) -> windows::core::Result<()> {
        if on_file_system(folder) {
            return Ok(());
        }
        if self.redirecting.replace(true) {
            return Ok(());
        }
        let moved = match profile_folder() {
            // SAFETY: COM calls on the dialog of this thread.
            Some(profile) => unsafe {
                item(&profile)
                    .and_then(|profile| dialog.SetFolder(&profile))
                    .and_then(|()| dialog.SetFileName(&HSTRING::new()))
            },
            None => {
                log::warn!(
                    "the profile folder is not known: the folder picker stays off the file system"
                );
                Ok(())
            }
        };
        self.redirecting.set(false);
        moved
    }
}

impl IFileDialogEvents_Impl for FileSystemOnly_Impl {
    fn OnFileOk(&self, _dialog: Ref<IFileDialog>) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnFolderChanging(
        &self,
        _dialog: Ref<IFileDialog>,
        _folder: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnFolderChange(&self, dialog: Ref<IFileDialog>) -> windows::core::Result<()> {
        // The notification names no dialog when it is null, and then there
        // is nothing to keep on the file system.
        let Some(dialog) = dialog.as_ref() else {
            return Ok(());
        };
        // SAFETY: COM calls on the dialog of this thread, in its own
        // notification.
        let folder = unsafe { dialog.GetFolder() }?;
        self.keep(dialog, &folder)
    }

    fn OnSelectionChange(&self, _dialog: Ref<IFileDialog>) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnShareViolation(
        &self,
        _dialog: Ref<IFileDialog>,
        _item: Ref<IShellItem>,
    ) -> windows::core::Result<FDE_SHAREVIOLATION_RESPONSE> {
        Ok(FDE_SHAREVIOLATION_RESPONSE::default())
    }

    fn OnTypeChange(&self, _dialog: Ref<IFileDialog>) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnOverwrite(
        &self,
        _dialog: Ref<IFileDialog>,
        _item: Ref<IShellItem>,
    ) -> windows::core::Result<FDE_OVERWRITE_RESPONSE> {
        Ok(FDE_OVERWRITE_RESPONSE::default())
    }
}

/// Sets what the open and save dialogs share.
fn configure(
    dialog: &IFileDialog,
    title: Option<&str>,
    button_label: Option<&str>,
    directory: Option<&PathBuf>,
    filters: &[FileFilter],
) -> windows::core::Result<()> {
    // SAFETY: COM calls on a dialog of this thread; the strings outlive
    // the calls that read them (the dialog copies them).
    unsafe {
        if let Some(title) = title {
            dialog.SetTitle(&HSTRING::from(title))?;
        }
        if let Some(label) = button_label {
            dialog.SetOkButtonLabel(&HSTRING::from(label))?;
        }
        if let Some(folder) = directory.and_then(|d| item(d).ok()) {
            dialog.SetFolder(&folder)?;
        }
        if !filters.is_empty() {
            // Each filter as the shell's dialogs show them:
            // `Markdown (*.md;*.markdown)`.
            let strings: Vec<(HSTRING, HSTRING)> = filters
                .iter()
                .map(|filter| {
                    let pattern = filter
                        .extensions
                        .iter()
                        .map(|e| format!("*.{e}"))
                        .collect::<Vec<_>>()
                        .join(";");
                    let name = format!("{} ({pattern})", filter.name);
                    (HSTRING::from(name), HSTRING::from(pattern))
                })
                .collect();
            let specs: Vec<COMDLG_FILTERSPEC> = strings
                .iter()
                .map(|(name, pattern)| COMDLG_FILTERSPEC {
                    pszName: PCWSTR(name.as_ptr()),
                    pszSpec: PCWSTR(pattern.as_ptr()),
                })
                .collect();
            dialog.SetFileTypes(&specs)?;
            if let Some(extension) = filters[0].extensions.first() {
                dialog.SetDefaultExtension(&HSTRING::from(extension.as_str()))?;
            }
        }
    }
    Ok(())
}

/// Shows `dialog` owned by `owner`: whether the user confirmed it (`false`
/// when cancelled).
///
/// # Safety
///
/// `dialog` belongs to this thread's COM apartment.
unsafe fn show(dialog: &IFileDialog, owner: HWND) -> windows::core::Result<bool> {
    // SAFETY: as the caller promises.
    match unsafe { dialog.Show(Some(owner)) } {
        Ok(()) => Ok(true),
        Err(e) if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => Ok(false),
        Err(e) => Err(e),
    }
}

/// `Picker.pickFiles`: the shell's open dialog for `request`, on a thread of
/// its own as every request's; the files picked are read there and answer
/// pick `id` as a root `Pick` event.
pub(crate) fn pick_files(surface: &Rc<Surface>, id: u32, request: &FilePickRequest) {
    let mut control = None;
    surface.control(|c| control = Some(c.clone()));
    let Some(control) = control else { return };
    // A window handle is an address the other thread only hands to the
    // dialog as its owner.
    let owner = surface.hwnd().0 as isize;
    let request = request.clone();
    let spawned = std::thread::Builder::new()
        .name("nsh-picker".into())
        .spawn(move || {
            let paths = with_com(|| pick_paths(HWND(owner as *mut _), &request.dialog()))
                .and_then(|paths| paths)
                .unwrap_or_else(|e| {
                    log::warn!("the file dialog failed: {e}");
                    Vec::new()
                });
            let files = read_picked_files(&paths, &request);
            control.dispatch_event(ROOT_ID, EventKind::Pick, EventPayload::Picked { id, files });
        });
    if let Err(e) = spawned {
        log::warn!("cannot show the file dialog: {e}");
    }
}

/// The paths picked in the dialog; none when it was cancelled. The
/// shell's dialog picks files or folders, not both: files and folders pick
/// files.
fn pick_paths(owner: HWND, request: &OpenDialog) -> windows::core::Result<Vec<PathBuf>> {
    // SAFETY: COM calls on a dialog this thread owns until it returns.
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let mut options = dialog.GetOptions()? | FOS_FORCEFILESYSTEM;
        let folders = !request.picks_files();
        options |= if folders {
            FOS_PICKFOLDERS
        } else {
            FOS_FILEMUSTEXIST
        };
        if request.multiple {
            options |= FOS_ALLOWMULTISELECT;
        }
        dialog.SetOptions(options)?;
        let filters: &[FileFilter] = if folders { &[] } else { &request.filters };
        configure(
            &dialog,
            request.title.as_deref(),
            request.button_label.as_deref(),
            request.directory.as_ref(),
            filters,
        )?;
        // A picker that answers folders keeps the dialog on the file
        // system ([`FileSystemOnly`]); it holds the sink until unadvised.
        let events: Option<IFileDialogEvents> = folders.then(|| FileSystemOnly::default().into());
        let cookie = events
            .as_ref()
            .map(|events| dialog.Advise(events))
            .transpose()?;
        let shown = show(&dialog, owner);
        if let Some(cookie) = cookie {
            dialog.Unadvise(cookie)?;
        }
        if !shown? {
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
fn pick_save_path(owner: HWND, request: &SaveDialog) -> windows::core::Result<Option<PathBuf>> {
    // SAFETY: COM calls on a dialog this thread owns until it returns; the
    // name outlives the call that reads it.
    unsafe {
        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
        dialog.SetOptions(dialog.GetOptions()? | FOS_FORCEFILESYSTEM)?;
        if let Some(name) = &request.file_name {
            dialog.SetFileName(&HSTRING::from(name.as_str()))?;
        }
        configure(
            &dialog,
            request.title.as_deref(),
            request.button_label.as_deref(),
            request.directory.as_ref(),
            &request.filters,
        )?;
        if !show(&dialog, owner)? {
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

#[cfg(test)]
mod tests {
    use windows::core::PWSTR;

    use super::*;

    /// The string the shell allocated, copied and freed.
    fn taken(pointer: PWSTR) -> String {
        // SAFETY: the name the shell allocated is freed once copied.
        unsafe {
            let text = pointer.to_string().unwrap_or_default();
            CoTaskMemFree(Some(pointer.0 as *const _));
            text
        }
    }

    /// A folder of the shell that has no file system path.
    fn this_pc() -> IShellItem {
        // SAFETY: a NUL-terminated shell namespace name that outlives the
        // call.
        unsafe {
            SHCreateItemFromParsingName(
                &HSTRING::from("shell:::{20D04FE0-3AEA-1069-A2D8-08002B30309D}"),
                None,
            )
            .expect("the shell has This PC")
        }
    }

    /// A folder picker of this thread's COM apartment. It never shows, so
    /// the test opens no window.
    fn folder_picker() -> windows::core::Result<IFileOpenDialog> {
        // SAFETY: COM calls on a dialog this thread owns until it is
        // dropped.
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(dialog.GetOptions()? | FOS_FORCEFILESYSTEM | FOS_PICKFOLDERS)?;
            Ok(dialog)
        }
    }

    #[test]
    fn a_folder_with_no_path_opens_the_profile() {
        let (folder, name) = with_com(|| {
            let dialog = folder_picker()?;
            // SAFETY: COM calls on the dialog of this test's apartment.
            unsafe {
                dialog.SetFileName(&HSTRING::from("typed"))?;
                FileSystemOnly::default().keep(&dialog, &this_pc())?;
                let folder = item_path(&dialog.GetFolder()?)?;
                Ok::<_, windows::core::Error>((folder, taken(dialog.GetFileName()?)))
            }
        })
        .and_then(|answer| answer)
        .expect("the folder picker moves to the profile");

        assert_eq!(folder, profile_folder());
        assert!(name.is_empty());
    }

    #[test]
    fn a_folder_with_a_path_keeps_the_name_typed() {
        let (before, after, name) = with_com(|| {
            let dialog = folder_picker()?;
            // SAFETY: COM calls on the dialog of this test's apartment.
            unsafe {
                dialog.SetFileName(&HSTRING::from("typed"))?;
                let before = item_path(&dialog.GetFolder()?)?;
                let profile = profile_folder().expect("the profile folder is known");
                FileSystemOnly::default().keep(&dialog, &item(&profile)?)?;
                let after = item_path(&dialog.GetFolder()?)?;
                Ok::<_, windows::core::Error>((before, after, taken(dialog.GetFileName()?)))
            }
        })
        .and_then(|answer| answer)
        .expect("a folder with a path is left alone");

        assert_eq!(before, after);
        assert_eq!(name, "typed");
    }
}

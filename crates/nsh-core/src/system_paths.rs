//! The system's file services for local paths: the open and save dialogs,
//! opening a file or folder in its default app, showing it in the file
//! manager and moving it to the trash ([`crate::element::pick_paths`],
//! [`crate::element::pick_save_path`], [`crate::element::open_path`],
//! [`crate::element::reveal_path`], [`crate::element::trash_path`]), as
//! GPUI's `prompt_for_paths`, `prompt_for_new_path`, `open_with_system` and
//! `reveal_path` are.
//!
//! Each request is a [`Command::SystemPath`](crate::Command::SystemPath) on
//! the root with a [`PathRequest`]; the platform carries it out (AppKit's
//! `NSOpenPanel`, `NSSavePanel`, `NSWorkspace` and `NSFileManager`; GTK's
//! `GtkFileDialog`, `GtkFileLauncher` and `g_file_trash`; the shell's
//! `IFileOpenDialog`, `IFileSaveDialog`, `ShellExecuteW`,
//! `SHOpenFolderAndSelectItems` and `IFileOperation`; GPUI's prompts and
//! openers) and answers with the root's
//! [`EventKind::SystemPath`](crate::EventKind::SystemPath), whose
//! [`EventPayload::SystemPath`](crate::EventPayload::SystemPath) carries the
//! paths and the error, if any ([`answer`], [`failed`]).
//!
//! Only platforms whose
//! [`Environment::system_paths`](crate::platform::Environment::system_paths)
//! is set get these commands (the desktops: AppKit, GTK, Win32, GPUI); on
//! the others (iOS, Android, the web) every request answers at once with
//! its "unsupported" value: no paths, `false`, or an error for the trash.
//!
//! These services are Rust's only: no module of the runtime exposes them to
//! an app's JS, so only native code (an app's own crate, `nsh-base`) asks,
//! which reaches the filesystem anyway, and no capability gates them. The
//! dialogs only give what the user chose.

use std::path::PathBuf;

use crate::EventPayload;

/// What the open dialog shows ([`crate::element::pick_paths`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct PathPickRequest {
    /// Folders are picked instead of files.
    pub folders: bool,
    /// The user may pick several.
    pub multiple: bool,
    /// The file extensions offered (`"md"`, without the dot), lower case;
    /// empty: any file. Ignored when picking folders.
    pub extensions: Vec<String>,
    /// The dialog's title; `None`: the system's.
    pub title: Option<String>,
    /// The folder the dialog starts in; `None`: the system's choice.
    pub directory: Option<PathBuf>,
}

/// What the save dialog shows ([`crate::element::pick_save_path`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct SavePathRequest {
    /// The file name suggested (`"Untitled.md"`); `None`: none.
    pub name: Option<String>,
    /// The file extensions the file may have (`"md"`, without the dot),
    /// lower case; empty: any.
    pub extensions: Vec<String>,
    /// The dialog's title; `None`: the system's.
    pub title: Option<String>,
    /// The folder the dialog starts in; `None`: the system's choice.
    pub directory: Option<PathBuf>,
}

/// What the platform is asked to do with local paths
/// ([`crate::Command::SystemPath`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PathRequest {
    /// Show the open dialog: answer with the paths picked, none when
    /// cancelled.
    Pick(PathPickRequest),
    /// Show the save dialog: answer with the path chosen, none when
    /// cancelled.
    Save(SavePathRequest),
    /// Open the file or folder in its default app: answer with no error
    /// when it opened.
    Open(PathBuf),
    /// Show the item selected in the file manager: answer with no error
    /// when it showed.
    Reveal(PathBuf),
    /// Move the item to the trash: answer with where it went when the
    /// platform says (none otherwise), or the error.
    Trash(PathBuf),
}

impl PathPickRequest {
    /// `extensions` as the dialogs take them: without leading dots or `*.`,
    /// lower case, empty ones left out.
    pub(crate) fn normalize(&mut self) {
        normalize(&mut self.extensions);
    }
}

impl SavePathRequest {
    /// `extensions` as the dialogs take them ([`PathPickRequest`]'s).
    pub(crate) fn normalize(&mut self) {
        normalize(&mut self.extensions);
    }
}

fn normalize(extensions: &mut Vec<String>) {
    for extension in extensions.iter_mut() {
        let trimmed = extension
            .trim()
            .trim_start_matches('*')
            .trim_start_matches('.');
        *extension = trimmed.to_lowercase();
    }
    extensions.retain(|e| !e.is_empty());
    let mut seen = Vec::with_capacity(extensions.len());
    extensions.retain(|e| {
        let new = !seen.contains(e);
        if new {
            seen.push(e.clone());
        }
        new
    });
}

/// Whether `path`'s extension is one of `extensions` (any when they are
/// empty), for platforms whose dialogs cannot filter (GPUI's).
pub fn has_extension(path: &std::path::Path, extensions: &[String]) -> bool {
    extensions.is_empty()
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// The answer to request `id` that succeeded with `paths`: those picked, the
/// one chosen to save to, where a trashed item went, or none.
pub fn answer(id: u32, paths: Vec<PathBuf>) -> EventPayload {
    EventPayload::SystemPath {
        id,
        paths,
        error: None,
    }
}

/// The answer to request `id` that failed, with why.
pub fn failed(id: u32, error: impl Into<String>) -> EventPayload {
    EventPayload::SystemPath {
        id,
        paths: Vec::new(),
        error: Some(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_lose_dots_and_case() {
        let mut request = PathPickRequest {
            extensions: vec![
                ".MD".into(),
                "*.markdown".into(),
                "".into(),
                "md".into(),
                " txt ".into(),
            ],
            ..Default::default()
        };
        request.normalize();
        assert_eq!(request.extensions, ["md", "markdown", "txt"]);
        assert!(has_extension(
            std::path::Path::new("/a/B.Markdown"),
            &request.extensions
        ));
        assert!(!has_extension(
            std::path::Path::new("/a/b.rs"),
            &request.extensions
        ));
        assert!(has_extension(std::path::Path::new("/a/b"), &[]));
    }
}

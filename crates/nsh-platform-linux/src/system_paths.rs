//! Local paths ([`nsh_core::system_paths`]): GTK's file dialog for opening
//! and saving (the desktop portal's where there is one), `GtkFileLauncher`
//! to open a file in its app or show it in the file manager (the portal's
//! `OpenURI`, else `GAppInfo` and the `org.freedesktop.FileManager1`
//! service), and GIO's trash (`g_file_trash`). Each is parented to the
//! surface's window and answered as a root `SystemPath` event when GTK
//! calls back.

use std::{path::PathBuf, rc::Rc};

use gtk4::{self as gtk, gio, glib, prelude::*};
use nsh_core::{
    EventKind, EventPayload, ROOT_ID,
    system_paths::{self, PathPickRequest, PathRequest, SavePathRequest},
};

use crate::surface::Surface;

/// Carries out request `id`.
pub(crate) fn request(surface: &Rc<Surface>, id: u32, request: &PathRequest) {
    let window = surface.root.root().and_downcast::<gtk::Window>();
    let surface = Rc::downgrade(surface);
    let request = request.clone();
    glib::spawn_future_local(async move {
        let answer = match request {
            PathRequest::Pick(pick) => system_paths::answer(id, pick_paths(&pick, window).await),
            PathRequest::Save(save) => system_paths::answer(
                id,
                pick_save_path(&save, window).await.into_iter().collect(),
            ),
            PathRequest::Open(path) => done(
                id,
                gtk::FileLauncher::new(Some(&gio::File::for_path(&path)))
                    .launch_future(window.as_ref())
                    .await,
            ),
            PathRequest::Reveal(path) => done(
                id,
                gtk::FileLauncher::new(Some(&gio::File::for_path(&path)))
                    .open_containing_folder_future(window.as_ref())
                    .await,
            ),
            PathRequest::Trash(path) => done(
                id,
                gio::File::for_path(&path)
                    .trash_future(glib::Priority::DEFAULT)
                    .await,
            ),
            other => {
                log::debug!("linux: path request {other:?} is not supported");
                system_paths::failed(id, "not supported")
            }
        };
        if let Some(surface) = surface.upgrade() {
            surface.event(ROOT_ID, EventKind::SystemPath, answer);
        }
    });
}

fn done(id: u32, result: Result<(), glib::Error>) -> EventPayload {
    match result {
        Ok(()) => system_paths::answer(id, Vec::new()),
        Err(err) => system_paths::failed(id, err.to_string()),
    }
}

/// A dialog showing `title`, starting in `directory`, offering
/// `extensions` (any file without).
fn dialog(
    title: Option<&str>,
    directory: Option<&PathBuf>,
    extensions: &[String],
) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder().modal(true).build();
    if let Some(title) = title {
        dialog.set_title(title);
    }
    if let Some(directory) = directory {
        dialog.set_initial_folder(Some(&gio::File::for_path(directory)));
    }
    if !extensions.is_empty() {
        // No label of NSH's own: the filter shows its patterns.
        let filter = gtk::FileFilter::new();
        for extension in extensions {
            filter.add_suffix(extension);
        }
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        dialog.set_default_filter(Some(&filter));
    }
    dialog
}

/// The paths picked; none when the dialog was dismissed or failed.
async fn pick_paths(request: &PathPickRequest, window: Option<gtk::Window>) -> Vec<PathBuf> {
    let extensions: &[String] = if request.folders {
        &[]
    } else {
        &request.extensions
    };
    let dialog = dialog(
        request.title.as_deref(),
        request.directory.as_ref(),
        extensions,
    );
    let window = window.as_ref();
    let list = match (request.folders, request.multiple) {
        (false, false) => one(dialog.open_future(window).await),
        (false, true) => many(dialog.open_multiple_future(window).await),
        (true, false) => one(dialog.select_folder_future(window).await),
        (true, true) => many(dialog.select_multiple_folders_future(window).await),
    };
    list.unwrap_or_else(|err| {
        dismissed_or_log(&err);
        Vec::new()
    })
}

/// The path chosen; none when the dialog was dismissed or failed.
async fn pick_save_path(request: &SavePathRequest, window: Option<gtk::Window>) -> Option<PathBuf> {
    let dialog = dialog(
        request.title.as_deref(),
        request.directory.as_ref(),
        &request.extensions,
    );
    if let Some(name) = &request.name {
        dialog.set_initial_name(Some(name));
    }
    match dialog.save_future(window.as_ref()).await {
        Ok(file) => file.path(),
        Err(err) => {
            dismissed_or_log(&err);
            None
        }
    }
}

fn one(result: Result<gio::File, glib::Error>) -> Result<Vec<PathBuf>, glib::Error> {
    result.map(|file| file.path().into_iter().collect())
}

fn many(result: Result<gio::ListModel, glib::Error>) -> Result<Vec<PathBuf>, glib::Error> {
    result.map(|list| {
        (0..list.n_items())
            .filter_map(|i| list.item(i).and_downcast::<gio::File>())
            .filter_map(|file| file.path())
            .collect()
    })
}

/// A dismissed dialog is the user's answer; anything else is logged.
fn dismissed_or_log(err: &glib::Error) {
    if !err.matches(gtk::DialogError::Dismissed) && !err.matches(gtk::DialogError::Cancelled) {
        log::warn!("linux: the file dialog failed: {err}");
    }
}

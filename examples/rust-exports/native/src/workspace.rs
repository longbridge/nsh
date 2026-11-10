//! An app plugin that depends on the official theme capability.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use nsh_core::{
    JsonValue,
    element::{Plugin, PluginDependency},
    prelude::*,
    runtime::ThemePlugin,
};

/// A workspace's palette, or the theme the app was built with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
#[non_exhaustive]
pub enum WorkspacePalette {
    /// Restore the theme from `nsh.json`.
    #[default]
    Built,
    /// Blue and cyan for the ocean workspace.
    Ocean,
    /// Purple and lilac for the orchid workspace.
    Orchid,
}

/// The app's workspace selection and its dependency on the theme provider.
#[derive(Default)]
#[non_exhaustive]
pub struct WorkspaceTheme {
    theme: RefCell<Option<Rc<ThemePlugin>>>,
    selected: Cell<WorkspacePalette>,
}

impl Plugin for WorkspaceTheme {
    const ID: &'static str = "rust-exports.workspace-theme";
    const REQUIRES: &'static [PluginDependency] = &[PluginDependency::new("nsh.theme", 1)];

    fn start(&self, system: &System) -> Result<(), String> {
        let theme = system
            .plugin::<ThemePlugin>()
            .map_err(|error| error.message().to_owned())?;
        self.theme.replace(Some(theme));
        Ok(())
    }

    fn stop(&self) {
        self.theme.take();
    }
}

impl WorkspaceTheme {
    fn select(&self, palette: WorkspacePalette, cx: CallCx<'_>) -> Result<(), JsError> {
        let theme = self.theme.borrow().clone().ok_or_else(|| {
            JsError::Error("Workspace.select: the workspace plugin has stopped".into())
        })?;
        let candidate = match palette {
            WorkspacePalette::Built => None,
            WorkspacePalette::Ocean => Some(palette_theme("#0369a1", "#67e8f9")),
            WorkspacePalette::Orchid => Some(palette_theme("#7e22ce", "#d8b4fe")),
        };
        // A token reader invoked during publication can read the selected
        // workspace. Refused publications restore the preceding selection.
        let before = self.selected.replace(palette);
        if let Err(error) = theme.set(candidate.as_ref(), cx) {
            self.selected.set(before);
            return Err(error);
        }
        Ok(())
    }
}

fn palette_theme(light: &str, dark: &str) -> JsonValue {
    let mut theme = JsonValue::default();
    theme["cssVars"]["light"]["workspace-accent"] = light.into();
    theme["cssVars"]["dark"]["workspace-accent"] = dark.into();
    theme
}

/// Workspace palettes supplied by the app's runtime plugin.
#[derive(Default)]
#[non_exhaustive]
#[module]
pub struct Workspace;

#[module]
impl Workspace {
    /// Selects a workspace palette, or restores the built theme with
    /// `"built"`. `token()` reads the result before this call returns.
    pub fn select(
        palette: WorkspacePalette,
        system: System,
        cx: CallCx<'_>,
    ) -> Result<(), JsError> {
        system.plugin::<WorkspaceTheme>()?.select(palette, cx)
    }

    /// The last accepted workspace selection in this engine.
    pub fn current(system: System) -> Result<WorkspacePalette, JsError> {
        Ok(system.plugin::<WorkspaceTheme>()?.selected.get())
    }
}

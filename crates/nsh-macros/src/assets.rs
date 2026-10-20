//! `asset!` and `assets!`: the files a crate ships in its `assets/`
//! directory, read at compile time (an image's natural size, a font's
//! family), as TS's imports of image and font files do.
//!
//! A crate's files are published under `asset:/@<package>/<path>` (`path`
//! relative to `<crate>/assets`), apart from the app's own files: `nsh run`
//! serves them and `nsh package` copies them into the app from the paths the
//! app's registry lists (`Registry::assets`).

use std::path::{Component, Path, PathBuf};

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

/// File types whose natural size an image asset carries (as the bundler's
/// image imports).
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "heic"];

/// File types registered as fonts.
const FONT_EXTENSIONS: &[&str] = &["ttf", "otf"];

/// What a file is, by its extension.
enum Kind {
    Image,
    Font,
    File,
}

fn kind(path: &Path) -> Kind {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Kind::Image
    } else if FONT_EXTENSIONS.contains(&ext.as_str()) {
        Kind::Font
    } else {
        Kind::File
    }
}

/// The calling crate's `assets/` directory and package name.
fn crate_assets(span: Span) -> syn::Result<(PathBuf, String)> {
    let dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| syn::Error::new(span, "assets are read in crates built by Cargo"))?;
    let package = std::env::var("CARGO_PKG_NAME")
        .map_err(|_| syn::Error::new(span, "assets are read in crates built by Cargo"))?;
    Ok((PathBuf::from(dir).join("assets"), package))
}

/// `logo@2x.png` -> 2.0: the pixel density encoded in the file name.
fn density(path: &Path) -> f64 {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    match stem.rsplit_once('@').and_then(|(_, d)| d.strip_suffix('x')) {
        Some(d) => d.parse().unwrap_or(1.0),
        None => 1.0,
    }
}

/// The family name of a font file.
fn font_family(data: &[u8]) -> Result<String, String> {
    let face = ttf_parser::Face::parse(data, 0).map_err(|e| e.to_string())?;
    let name = |id| {
        face.names()
            .into_iter()
            .filter(|n| n.name_id == id && n.is_unicode())
            .find_map(|n| n.to_string())
    };
    name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
        .or_else(|| name(ttf_parser::name_id::FAMILY))
        .ok_or_else(|| "the font has no family name".to_owned())
}

/// `rel` as a published path: `/`-separated, inside the directory.
fn published(rel: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for c in rel.components() {
        match c {
            Component::Normal(part) => parts.push(part.to_str()?),
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The file's path as a string literal (`include_bytes!` makes Cargo build
/// the crate again when the file changes).
fn source(file: &Path, span: Span) -> syn::Result<String> {
    file.to_str()
        .map(str::to_owned)
        .ok_or_else(|| syn::Error::new(span, "the asset's path is not UTF-8"))
}

/// `asset!("logo@2x.png")`.
pub(crate) fn asset(lit: LitStr) -> syn::Result<TokenStream> {
    let span = lit.span();
    let (dir, package) = crate_assets(span)?;
    let rel = lit.value();
    let path = published(Path::new(&rel)).ok_or_else(|| {
        syn::Error::new(
            span,
            "an asset is a path inside the crate's `assets/` directory",
        )
    })?;
    let file = dir.join(&rel);
    if !file.is_file() {
        return Err(syn::Error::new(
            span,
            format!(
                "no asset {} (assets are in the crate's `assets/` directory)",
                file.display()
            ),
        ));
    }
    let uri = format!("asset:/@{package}/{path}");
    let source = source(&file, span)?;
    let ui = quote!(::nsh_core::ui);
    Ok(match kind(&file) {
        Kind::Image => {
            let dim = imagesize::size(&file).map_err(|e| {
                syn::Error::new(span, format!("cannot read image {}: {e}", file.display()))
            })?;
            let scale = density(&file);
            let width = (dim.width as f64 / scale) as f32;
            let height = (dim.height as f64 / scale) as f32;
            quote! {{
                const _: &[u8] = ::core::include_bytes!(#source);
                #ui::ImageAsset {
                    uri: #uri,
                    size: ::nsh_core::geometry::size(
                        ::nsh_core::geometry::Points(#width),
                        ::nsh_core::geometry::Points(#height),
                    ),
                }
            }}
        }
        Kind::Font => {
            let data = std::fs::read(&file).map_err(|e| syn::Error::new(span, e.to_string()))?;
            let family = font_family(&data).map_err(|e| {
                syn::Error::new(span, format!("cannot read font {}: {e}", file.display()))
            })?;
            quote! {{
                const _: &[u8] = ::core::include_bytes!(#source);
                #ui::FontAsset { uri: #uri, family: #family }
            }}
        }
        Kind::File => quote!(#ui::FileAsset { uri: #uri }),
    })
}

/// Every file under `dir`, sorted; hidden files (`.DS_Store`) left out.
fn files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// `assets!()`.
pub(crate) fn assets(span: Span) -> syn::Result<TokenStream> {
    let (dir, package) = crate_assets(span)?;
    let mut found = Vec::new();
    if dir.is_dir() {
        files(&dir, &mut found)
            .map_err(|e| syn::Error::new(span, format!("{}: {e}", dir.display())))?;
    }
    let ui = quote!(::nsh_core::ui);
    let mut entries = Vec::new();
    for file in &found {
        let rel = file.strip_prefix(&dir).expect("listed under the directory");
        let Some(path) = published(rel) else {
            return Err(syn::Error::new(
                span,
                format!("{}: the asset's path is not UTF-8", file.display()),
            ));
        };
        let uri = format!("asset:/@{package}/{path}");
        let source = source(file, span)?;
        let variant = match kind(file) {
            Kind::Image => quote!(Image),
            Kind::Font => quote!(Font),
            Kind::File => quote!(File),
        };
        entries.push(quote! {
            #ui::AssetFile { uri: #uri, source: #source, kind: #ui::AssetKind::#variant }
        });
    }
    Ok(quote! {{
        const FILES: &[#ui::AssetFile] = &[#(#entries),*];
        FILES
    }})
}

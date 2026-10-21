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
    let (dir, package) = crate_assets(lit.span())?;
    asset_in(&dir, &package, lit)
}

/// `asset!` of package `package`, whose files are in `dir`.
fn asset_in(dir: &Path, package: &str, lit: LitStr) -> syn::Result<TokenStream> {
    let span = lit.span();
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
    assets_in(&dir, &package, span)
}

/// `assets!()` of package `package`, whose files are in `dir`.
fn assets_in(dir: &Path, package: &str, span: Span) -> syn::Result<TokenStream> {
    let mut found = Vec::new();
    if dir.is_dir() {
        files(dir, &mut found)
            .map_err(|e| syn::Error::new(span, format!("{}: {e}", dir.display())))?;
    }
    let ui = quote!(::nsh_core::ui);
    let mut entries = Vec::new();
    for file in &found {
        let rel = file.strip_prefix(dir).expect("listed under the directory");
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The start of a PNG of `width` × `height` pixels: what is read of it.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        data.extend(width.to_be_bytes());
        data.extend(height.to_be_bytes());
        data.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        data
    }

    /// The start of a GIF of `width` × `height` pixels.
    fn gif(width: u16, height: u16) -> Vec<u8> {
        let mut data = b"GIF89a".to_vec();
        data.extend(width.to_le_bytes());
        data.extend(height.to_le_bytes());
        data.extend([0, 0, 0]);
        data
    }

    /// A crate's `assets/` directory of its own, under the temporary
    /// directory: a 2× logo, a GIF, a font and its licence in `fonts/`,
    /// and a file the system left there.
    fn crate_assets(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nsh-macros-assets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("fonts")).unwrap();
        for (path, data) in [
            ("logo@2x.png", png(128, 64)),
            ("bounce.gif", gif(96, 96)),
            ("fonts/Script.ttf", b"not a font".to_vec()),
            ("fonts/OFL.txt", b"license".to_vec()),
            (".DS_Store", Vec::new()),
        ] {
            std::fs::write(dir.join(path), data).unwrap();
        }
        dir
    }

    fn lit(path: &str) -> LitStr {
        LitStr::new(path, Span::call_site())
    }

    /// The code's text without the spaces tokens print between them.
    fn code(tokens: TokenStream) -> String {
        tokens.to_string().replace(' ', "")
    }

    #[test]
    fn an_image_carries_its_natural_size_in_points() {
        let dir = crate_assets("image");
        // Two pixels per point: 128 × 64 pixels are 64 × 32 points.
        let logo = code(asset_in(&dir, "app", lit("logo@2x.png")).unwrap());
        assert!(
            logo.contains("ImageAsset{uri:\"asset:/@app/logo@2x.png\""),
            "{logo}"
        );
        assert!(
            logo.contains("Points(64f32)") && logo.contains("Points(32f32)"),
            "{logo}"
        );
        let bounce = code(asset_in(&dir, "app", lit("bounce.gif")).unwrap());
        assert!(bounce.contains("Points(96f32)"), "{bounce}");
        // A file of another kind is only its URI.
        let license = code(asset_in(&dir, "app", lit("fonts/OFL.txt")).unwrap());
        assert!(
            license.contains("FileAsset{uri:\"asset:/@app/fonts/OFL.txt\"}"),
            "{license}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_cannot_be_read_fails_the_build() {
        let dir = crate_assets("errors");
        let error = |path: &str| asset_in(&dir, "app", lit(path)).unwrap_err().to_string();
        assert!(error("../logo@2x.png").contains("inside the crate's `assets/`"));
        assert!(error("missing.png").starts_with("no asset"));
        assert!(error("fonts/Script.ttf").starts_with("cannot read font"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crate_ships_its_files_sorted_by_path_with_their_kinds() {
        let dir = crate_assets("list");
        let files = code(assets_in(&dir, "app", Span::call_site()).unwrap());
        let listed: Vec<(&str, &str)> = files
            .split("AssetFile{uri:\"asset:/@app/")
            .skip(1)
            .map(|entry| {
                let path = &entry[..entry.find('"').unwrap()];
                let kind = entry.split("AssetKind::").nth(1).unwrap();
                (path, &kind[..kind.find('}').unwrap()])
            })
            .collect();
        // The system's hidden file is left out.
        assert_eq!(
            listed,
            [
                ("bounce.gif", "Image"),
                ("fonts/OFL.txt", "File"),
                ("fonts/Script.ttf", "Font"),
                ("logo@2x.png", "Image"),
            ]
        );
        // Each one's source is the file on this system.
        let source = dir.join("logo@2x.png");
        assert!(
            files.contains(&format!("source:{:?}", source.to_str().unwrap()).replace(' ', "")),
            "{files}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

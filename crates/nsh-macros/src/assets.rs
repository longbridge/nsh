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

/// The pixel densities an image can ship in besides 1x, as
/// `nsh_core::IMAGE_SCALES` lists them (a proc macro cannot read it).
const VARIANT_SCALES: [f32; 4] = [1.5, 2.0, 3.0, 4.0];

/// The pixel-density variants of image `file` (its siblings
/// `<stem>@<scale>x.<ext>`), when its name has no density suffix. Stat
/// calls only, as the bundler's image imports look for them.
fn variants(file: &Path) -> Vec<(f32, PathBuf)> {
    let (Some(stem), Some(ext)) = (
        file.file_stem().and_then(|s| s.to_str()),
        file.extension().and_then(|e| e.to_str()),
    ) else {
        return Vec::new();
    };
    let suffixed = stem
        .rsplit_once('@')
        .and_then(|(_, d)| d.strip_suffix('x'))
        .is_some_and(|d| d.parse::<f64>().is_ok());
    if suffixed {
        return Vec::new();
    }
    VARIANT_SCALES
        .into_iter()
        .map(|s| (s, file.with_file_name(format!("{stem}@{s}x.{ext}"))))
        .filter(|(_, p)| p.is_file())
        .collect()
}

/// Checks that the `scale` variant `file` of an image of `points` (its
/// size in points) is an image of the points times the scale (to the
/// pixel), as the bundler does for imports. The bundler warns about a
/// variant of another size; a proc macro has no warnings, so here it is
/// an error, as an unreadable variant is in both.
fn check_variant(file: &Path, points: (f64, f64), scale: f32) -> Result<(), String> {
    let dim =
        imagesize::size(file).map_err(|e| format!("cannot read image {}: {e}", file.display()))?;
    let scale = f64::from(scale);
    let expected = (points.0 * scale, points.1 * scale);
    let off = |want: f64, got: usize| (want - got as f64).abs() >= 1.0;
    if off(expected.0, dim.width) || off(expected.1, dim.height) {
        return Err(format!(
            "{}: {}x{} pixels, expected {}x{} ({}x{} points at @{scale}x)",
            file.display(),
            dim.width,
            dim.height,
            expected.0,
            expected.1,
            points.0,
            points.1
        ));
    }
    Ok(())
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

/// `asset!(lit)` in a crate named `package` whose assets are in `dir`.
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
            let (w, h) = (dim.width as f64 / scale, dim.height as f64 / scale);
            let (width, height) = (w as f32, h as f32);
            let variants = variants(&file);
            for (scale, variant) in &variants {
                check_variant(variant, (w, h), *scale)
                    .map_err(|message| syn::Error::new(span, message))?;
            }
            let scales: Vec<f32> = if variants.is_empty() {
                Vec::new()
            } else {
                std::iter::once(1.0)
                    .chain(variants.iter().map(|(s, _)| *s))
                    .collect()
            };
            // Cargo builds the crate again when a variant changes.
            let variant_sources = variants
                .iter()
                .map(|(_, p)| self::source(p, span))
                .collect::<syn::Result<Vec<_>>>()?;
            quote! {{
                const _: &[u8] = ::core::include_bytes!(#source);
                #(const _: &[u8] = ::core::include_bytes!(#variant_sources);)*
                #ui::ImageAsset {
                    uri: #uri,
                    size: ::nsh_core::geometry::size(
                        ::nsh_core::geometry::Points(#width),
                        ::nsh_core::geometry::Points(#height),
                    ),
                    scales: &[#(#scales),*],
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The header of a `w`×`h` PNG: all the image size reader looks at.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        out.extend(w.to_be_bytes());
        out.extend(h.to_be_bytes());
        out.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        out
    }

    /// An `assets/` directory with `files` (name, pixel size).
    fn dir(name: &str, files: &[(&str, (u32, u32))]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsh-asset-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (file, (w, h)) in files {
            std::fs::write(dir.join(file), png(*w, *h)).unwrap();
        }
        dir
    }

    fn expand_or_fail(dir: &Path, rel: &str) -> syn::Result<String> {
        asset_in(dir, "demo", LitStr::new(rel, Span::call_site())).map(|t| t.to_string())
    }

    fn expand(dir: &Path, rel: &str) -> String {
        expand_or_fail(dir, rel).unwrap()
    }

    /// The `size` field the macro writes for `w`×`h` points.
    fn size(w: &str, h: &str) -> String {
        format!(
            "size : :: nsh_core :: geometry :: size (:: nsh_core :: geometry :: Points ({w}) , \
             :: nsh_core :: geometry :: Points ({h}) ,)"
        )
    }

    #[test]
    fn asset_variants_are_listed_and_tracked() {
        let dir = dir(
            "variants",
            &[
                ("logo.png", (40, 30)),
                ("logo@2x.png", (80, 60)),
                ("logo@3x.png", (120, 90)),
                ("other@4x.png", (4, 4)),
            ],
        );
        let out = expand(&dir, "logo.png");
        assert!(out.contains("uri : \"asset:/@demo/logo.png\""), "{out}");
        assert!(out.contains(&size("40f32", "30f32")), "{out}");
        assert!(out.contains("scales : & [1f32 , 2f32 , 3f32]"), "{out}");
        // Cargo builds again when a variant changes.
        for file in ["logo.png", "logo@2x.png", "logo@3x.png"] {
            let path = dir.join(file).to_string_lossy().into_owned();
            assert!(
                out.contains(&format!("include_bytes ! ({path:?})")),
                "{file}: {out}"
            );
        }
        let scales: Vec<f32> = variants(&dir.join("logo.png"))
            .iter()
            .map(|(s, _)| *s)
            .collect();
        assert_eq!(scales, [2.0, 3.0]);
    }

    #[test]
    fn asset_variants_none_for_a_single_file_or_a_suffixed_name() {
        let dir = dir(
            "single",
            &[
                ("one.png", (10, 10)),
                ("logo@2x.png", (80, 60)),
                ("logo@3x.png", (120, 90)),
            ],
        );
        let one = expand(&dir, "one.png");
        assert!(one.contains("scales : & []"), "{one}");
        let suffixed = expand(&dir, "logo@2x.png");
        assert!(suffixed.contains(&size("40f32", "30f32")), "{suffixed}");
        assert!(suffixed.contains("scales : & []"), "{suffixed}");
        assert!(!suffixed.contains("logo@3x.png"), "{suffixed}");
    }

    /// A variant that is no image, or an image of another size than its
    /// scale says, fails the build, naming the file.
    #[test]
    fn asset_variants_that_do_not_fit_fail_the_build() {
        let dir = dir("broken", &[("a.png", (40, 30)), ("b.png", (41, 31))]);
        std::fs::write(dir.join("a@2x.png"), b"not an image").unwrap();
        let error = expand_or_fail(&dir, "a.png").unwrap_err().to_string();
        assert!(error.starts_with("cannot read image "), "{error}");
        assert!(error.contains("a@2x.png"), "{error}");

        std::fs::write(dir.join("a@2x.png"), png(120, 90)).unwrap();
        let error = expand_or_fail(&dir, "a.png").unwrap_err().to_string();
        assert!(
            error.ends_with("a@2x.png: 120x90 pixels, expected 80x60 (40x30 points at @2x)"),
            "{error}"
        );

        // 1.5x of an odd size: either rounding fits.
        for pixels in [(61, 46), (62, 47)] {
            std::fs::write(dir.join("b@1.5x.png"), png(pixels.0, pixels.1)).unwrap();
            let out = expand(&dir, "b.png");
            assert!(out.contains("scales : & [1f32 , 1.5f32]"), "{out}");
        }
        std::fs::write(dir.join("b@1.5x.png"), png(60, 46)).unwrap();
        assert!(expand_or_fail(&dir, "b.png").is_err());
    }
}

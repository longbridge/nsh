# Images

- **Images**: background NSURLSession / file read → ImageIO downsampling decode at the view's pixel size → `NSCache` (purged under memory pressure);
  requests for the same key are coalesced and downloads nobody waits for are cancelled (Android: HttpURLConnection / APK assets →
  `BitmapFactory` power-of-two sampling → `LruCache`, same coalescing). During development `asset:/path` goes through the dev
  server's `/assets/`; `nsh build` copies assets to `out/assets/`, `nsh package` puts them in the app. Cache keys include the decode
  size and the blur radius. `blurRadius` is a stack blur on the decoding thread (`blur.rs`, `Blur.java`: the same algorithm, in
  pixels). `placeholder` loads after the image request and shows until the image arrives; the image then fades in over
  `fadeDuration` (iOS: `CATransition`; Android: alpha over the placeholder; default 0 on iOS and 300ms on Android, as in React
  Native). Animated images: iOS decodes up to 600 frames into an animated `UIImage` (frames repeated by their delays' common
  divisor) played by a discrete `CAKeyframeAnimation` on `contents` (a tinted animated image shows its first frame); Android
  (API 28+) uses `ImageDecoder` → `AnimatedImageDrawable`, not cached (each view plays its own copy; no blur). `onLoad` reports the
  source's size in pixels on both platforms, whatever size the view decodes at. Downloads are shared per URI (also between
  views decoding at different sizes) and report `onProgress` at most every 100ms; `Image.getSize` / `Image.prefetch` (Rust,
  `nsh_core::runtime`) ask with `Command::ImageInfo`, are answered by the root's `ImageInfo` event (which the core
  resolves, never JS) and keep the downloaded bytes in a data cache (32 MB) until a view
  decodes them. Android `ImageBox` draws its borders above the
  image (as Core Animation's layer border).
- **Pixel-density variants**: an `ImageSource` carries `scales` (`ImageScales`, a `Copy` bitmask over `IMAGE_SCALES`,
  `[1, 1.5, 2, 3, 4]`) when a bundled asset ships `@Nx` siblings (of a JS `scales` list, the first `SCALES_READ`
  entries). The core picks the variant when it emits `MountItem::Image` (`ImageSource::uri_for`, for the source and the
  `placeholder`): React Native's rule, the smallest scale at least the surface's `pixel_ratio`, otherwise
  the largest, for `asset:/` URIs without a query or a fragment (a bundled file's name has neither). The backends load
  that URI as any other, so their caches and decoding are unchanged; the size in points still comes from the source, so
  layout never depends on the variant. When the pixel ratio changes (`apply_environment`: a window moved to another
  screen, browser zoom) only the images whose pick changes are marked `dirty::IMAGE` and sent again, with no relayout.
  The backends load a re-sent image as a new source, with its events: `onLoadStart`, `onLoad` (the variant's pixels, as
  for any source) or `onError`, then `onLoadEnd`. `Image.getSize` / `Image.prefetch` pick the same way, so a prefetch
  warms the bytes the views load; `getSize` returns the imported dimensions after a successful load, including when a variant has mismatched
  dimensions. Without imported dimensions it divides the variant's pixels by its scale.
  The bundler finds the variants: its image `load` hook (`plugin.rs`) stats the fixed names `<stem>@<s>x.<ext>` for the
  scales of `IMAGE_SCALES` above 1 (no directory listing), only for bitmap imports without a density suffix
  (`logo@3x.png` probes nothing and ships alone; an SVG, drawn for the screen at any ratio, ships no variants). The ones
  found join `Bundle::assets`, so the dev server, `out/assets`, every package, `.nshpkg` and the web package's asset map
  carry them, and the module gains `scales` (1 first) only when there is one: an app without variants bundles byte for
  byte as before. Width and height still come from the imported file; a variant that cannot be read fails the build as
  the imported file does, and one whose pixels are off that size times its scale by a pixel or more is warned about on
  `nsh::build`.
  Rust's `asset!` does the same for a crate image (`nsh-macros` `assets.rs`): `ImageAsset::scales` lists the siblings it
  finds (`assets!()` already ships the whole directory) and `include_bytes!`s them, so a changed variant rebuilds the
  crate. It reads each variant's size: one that cannot be read, or whose pixels are off, fails the build (a proc macro
  has errors only, where the bundler warns about the size). A proc macro cannot read nsh-core, so its `VARIANT_SCALES`
  repeats `IMAGE_SCALES` above 1: change them together.
- **SVG** (`nsh_core::svg`, cargo feature `svg`, which every native platform crate enables and `nsh-platform-web` never
  does: browsers draw SVG with `<img>`, and the tint mask works on it as on bitmaps). The decoders sniff the bytes
  (`svg::sniff`: `<svg` after a BOM, XML declaration, comments and doctype, or gzip) before their platform decoder and
  draw with resvg. iOS, macOS and Android draw for the requested pixel size, the view's longer side (the size bitmaps
  are downsampled to), with the SVG's shorter side that long (`svg::rasterize_covering`: shown in the view, an image's
  shorter side never exceeds it, so no resize mode, `cover` included, enlarges the raster; Android through
  `Native.svg`), and an image at its natural size at the screen's scale (`svg::rasterize_scaled`; `center` uses the SVG's
  intrinsic dimensions with the platform's existing fitting rules: iOS through the `UIImage`'s scale, Android
  through `ImageBox`, with unrounded dimensions carried separately from the whole-point load reports over JNI). Windows, Linux and GPUI
  draw once at `svg::OVERSAMPLE` (4) pixels per point, at most `svg::MAX_OVERSAMPLED_SIDE` (2048) a side
  (`svg::rasterize_oversampled`), since they cache one bitmap per location. The premultiplied RGBA raster then takes
  the bitmap path (blur, color matrix, tint through alpha, cache). `onLoad` and `Image.getSize` report the intrinsic
  size in whole points (`svg::size`: `width` / `height`, one of them plus the `viewBox`'s ratio, or the `viewBox`;
  `svg::reported_size` rounds it), measured off the main thread; layout and fitting use it unrounded, and a file
  name's `@2x` does not apply. The bundler sizes `.svg` imports with the same `svg::size` and warns (`nsh::build`)
  about `<text>`, animation (SMIL elements, CSS `animation` and `@keyframes`), `<image>` and `<foreignObject>`, which
  only browsers draw. Canvases draw bitmaps: their image loads refuse an SVG with a warning naming it. Like bitmaps,
  per-view backends draw an SVG at the view's size when it loads: a view that grows later scales that raster until
  the image loads again.
- **SVG is untrusted input** (a remote SVG is a document from the network), so `nsh_core::svg` bounds what reading and
  drawing one takes. It reads nothing outside the document: usvg gets no resources directory and an `<image>` resolver
  that resolves nothing (its default one reads files), and an empty font database, so `<text>` is not drawn; the main
  workspace also builds resvg without `text`, `system-fonts` and `raster-images` (GPUI's workspace turns them on for
  GPUI, where the resolver and the empty font database still leave text and embedded images out, as the GPUI backend's
  `svg_` tests check). A document is at most `svg::MAX_BYTES` (16 MiB; a `.svgz` inflates that far and no further). A
  raster is at most `svg::MAX_SIDE` (8192) pixels a side and `svg::MAX_PIXELS` (16 Mi, 64 MiB of RGBA: a full-screen
  image on a 5K display fits) in all; a larger one is drawn smaller. resvg draws each isolated group (opacity, a mask,
  a clip path, filters, a blend mode) into a layer of its own, up to 5 × the canvas each way, and nested layers live
  at once, so a few hundred bytes of nested groups would take gigabytes: before drawing, `svg/layers.rs` walks the
  tree as resvg 0.46 draws it and adds up, along every nesting path, the layers, clip and mask pixmaps, filter regions
  (a pixmap per primitive, not clamped) and pattern tiles (not clamped) alive at once, and an SVG over
  `svg::MAX_LAYER_PIXELS` (64 Mi, four rasters' worth) or whose drawing visits more than a million nodes (shared
  patterns count each time they are drawn) is refused with an `svg::Error`. A panic in usvg or resvg is an
  `svg::Error` too (`catch_unwind`): the decoders run in GCD blocks and JNI functions, where an unwinding panic aborts.
  Android's decode threads also catch `OutOfMemoryError`, so a load that runs out of memory fails instead of hanging.

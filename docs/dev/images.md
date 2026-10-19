# Images

- **Images**: background NSURLSession / file read → ImageIO downsampling decode at the view's pixel size → `NSCache` (purged under memory pressure);
  requests for the same key are coalesced and downloads nobody waits for are cancelled (Android: HttpURLConnection / APK assets →
  `BitmapFactory` power-of-two sampling → `LruCache`, same coalescing). During development `asset:/path` goes through the dev
  server's `/assets/`; `nsh build` copies assets to `out/assets/`, `nsh package` puts them in the app. Cache keys include the decode
  size and the blur radius. `blurRadius` is a stack blur on the decoding thread (`blur.rs`, `Blur.java`: the same algorithm, in
  pixels). `defaultSource` loads after the image request and shows until the image arrives; the image then fades in over
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
  `defaultSource` placeholder): React Native's rule, the smallest scale at least the surface's `pixel_ratio`, otherwise
  the largest, for `asset:/` URIs without a query or a fragment (a bundled file's name has neither). The backends load
  that URI as any other, so their caches and decoding are unchanged; the size in points still comes from the source, so
  layout never depends on the variant. When the pixel ratio changes (`apply_environment`: a window moved to another
  screen, browser zoom) only the images whose pick changes are marked `dirty::IMAGE` and sent again, with no relayout.
  The backends load a re-sent image as a new source, with its events: `onLoadStart`, `onLoad` (the variant's pixels, as
  for any source) or `onError`, then `onLoadEnd`. `Image.getSize` / `Image.prefetch` pick the same way, so a prefetch
  warms the bytes the views load; `getSize` divides the variant's pixels by its scale, the imported file's size.
  The bundler finds the variants: its image `load` hook (`plugin.rs`) stats the fixed names `<stem>@<s>x.<ext>` for the
  scales of `IMAGE_SCALES` above 1 (no directory listing), only for imports without a density suffix (`logo@3x.png`
  probes nothing and ships alone). The ones found join `Bundle::assets`, so the dev server, `out/assets`, every package,
  `.nshpkg` and the web package's asset map carry them, and the module gains `scales` (1 first) only when there is one:
  an app without variants bundles byte for byte as before. Width and height still come from the imported file; a variant
  that cannot be read fails the build as the imported file does, and one whose pixels are off that size times its scale
  by a pixel or more is warned about on `nsh::build`.
  Rust's `asset!` does the same for a crate image (`nsh-macros` `assets.rs`): `ImageAsset::scales` lists the siblings it
  finds (`assets!()` already ships the whole directory) and `include_bytes!`s them, so a changed variant rebuilds the
  crate. It reads each variant's size: one that cannot be read, or whose pixels are off, fails the build (a proc macro
  has errors only, where the bundler warns about the size). A proc macro cannot read nsh-core, so its `VARIANT_SCALES`
  repeats `IMAGE_SCALES` above 1: change them together.

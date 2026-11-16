//! What the macOS and iOS backends read alike from an ImageIO source: which
//! of its images to show, its size and its frames' delays.
//!
//! A file of several images is an animation only in a format that animates
//! (GIF, APNG, animated WebP, HEIF and AVIF sequences, JPEG XL): every frame
//! plays. Other formats hold one picture at several sizes (ICO, ICNS) or
//! several pages (TIFF, HEIC collections), and show one image, the one that
//! fits the size shown, as browsers show a favicon.

use std::ops::Range;

use objc2_core_foundation::{CFDictionary, CFNumber, CFString, CFType};
use objc2_image_io::CGImageSource;

/// The type identifiers of the formats whose images, when a file holds
/// several, are the frames of an animation.
const ANIMATIONS: &[&str] = &[
    "com.compuserve.gif",
    "public.png",
    "org.webmproject.webp",
    "public.heics",
    "public.avis",
    "public.avif",
    "public.jpeg-xl",
];

/// The indices of the images of `source` to decode: the frames of an
/// animation (at most `max_frames`), or the one image of a multi-size or
/// multi-page file that fits `longest` pixels (0: the largest).
pub fn shown(source: &CGImageSource, longest: u32, max_frames: usize) -> Range<usize> {
    if is_animation(source) {
        // SAFETY: a plain query of a valid source.
        return 0..unsafe { source.count() }.clamp(1, max_frames.max(1));
    }
    let still = still(source, longest);
    still..still + 1
}

/// Whether `source` is an animation: several images in a format that
/// animates.
pub fn is_animation(source: &CGImageSource) -> bool {
    // SAFETY: plain queries of a valid source.
    let (count, kind) = unsafe { (source.count(), source.r#type()) };
    count > 1 && kind.is_some_and(|kind| ANIMATIONS.contains(&kind.to_string().as_str()))
}

/// The image of a still `source` to show `longest` pixels long: the smallest
/// at least that long, or the largest when none is (or `longest` is 0); the
/// deeper of two the same size.
pub fn still(source: &CGImageSource, longest: u32) -> usize {
    // SAFETY: a plain query of a valid source.
    let count = unsafe { source.count() };
    if count <= 1 {
        return 0;
    }
    let images: Vec<(usize, u32, u32)> = (0..count)
        .map(|i| {
            let (w, h) = image_size(source, i);
            (i, w.max(h), number(source, i, depth_key()))
        })
        .collect();
    let largest = images
        .iter()
        .max_by_key(|(i, side, depth)| (*side, *depth, std::cmp::Reverse(*i)));
    let fitting = images
        .iter()
        .filter(|(_, side, _)| longest > 0 && *side >= longest)
        .min_by_key(|(i, side, depth)| (*side, std::cmp::Reverse(*depth), *i));
    fitting.or(largest).map_or(0, |(i, ..)| *i)
}

/// The natural pixel size of `source`, as displayed (EXIF orientations 5–8
/// turn it a quarter): its first frame's, or its largest image's when it
/// holds one picture at several sizes.
pub fn source_size(source: &CGImageSource) -> (u32, u32) {
    let index = match is_animation(source) {
        true => 0,
        false => still(source, 0),
    };
    image_size(source, index)
}

/// The pixel size of image `index` of `source`, as displayed (EXIF
/// orientations 5–8 turn it a quarter).
pub fn image_size(source: &CGImageSource, index: usize) -> (u32, u32) {
    use objc2_image_io::{
        kCGImagePropertyOrientation, kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
    };
    // SAFETY: constants of ImageIO.
    let (width, height, orientation) = unsafe {
        (
            kCGImagePropertyPixelWidth,
            kCGImagePropertyPixelHeight,
            kCGImagePropertyOrientation,
        )
    };
    let (w, h) = (number(source, index, width), number(source, index, height));
    match number(source, index, orientation) {
        5..=8 => (h, w),
        _ => (w, h),
    }
}

fn depth_key() -> &'static CFString {
    // SAFETY: a constant of ImageIO.
    unsafe { objc2_image_io::kCGImagePropertyDepth }
}

/// A whole-number property of image `index` (0 when it has none).
fn number(source: &CGImageSource, index: usize, key: &CFString) -> u32 {
    // SAFETY: a CoreFoundation dictionary of CFString keys, read with a
    // documented ImageIO key and value type.
    unsafe {
        let Some(props) = source.properties_at_index(index, None) else {
            return 0;
        };
        let props: &CFDictionary<CFString, CFType> = props.cast_unchecked();
        props
            .get(key)
            .and_then(|v| v.downcast_ref::<CFNumber>().and_then(|n| n.as_i64()))
            .unwrap_or(0)
            .clamp(0, u32::MAX as i64) as u32
    }
}

/// The delay of frame `i` in centiseconds (GIF's unit), at least 2 (as
/// browsers: shorter ones show for 10).
pub fn frame_delay(source: &CGImageSource, i: usize) -> u32 {
    use objc2_image_io::{
        kCGImagePropertyAPNGDelayTime, kCGImagePropertyAPNGUnclampedDelayTime,
        kCGImagePropertyGIFDelayTime, kCGImagePropertyGIFDictionary,
        kCGImagePropertyGIFUnclampedDelayTime, kCGImagePropertyHEICSDelayTime,
        kCGImagePropertyHEICSDictionary, kCGImagePropertyHEICSUnclampedDelayTime,
        kCGImagePropertyPNGDictionary, kCGImagePropertyWebPDelayTime,
        kCGImagePropertyWebPDictionary, kCGImagePropertyWebPUnclampedDelayTime,
    };
    // SAFETY: CoreFoundation dictionaries of CFString keys, read with the
    // documented ImageIO keys and value types.
    let seconds = unsafe {
        source.properties_at_index(i, None).and_then(|props| {
            let props: &CFDictionary<CFString, CFType> = props.cast_unchecked();
            [
                (
                    kCGImagePropertyGIFDictionary,
                    kCGImagePropertyGIFUnclampedDelayTime,
                    kCGImagePropertyGIFDelayTime,
                ),
                (
                    kCGImagePropertyPNGDictionary,
                    kCGImagePropertyAPNGUnclampedDelayTime,
                    kCGImagePropertyAPNGDelayTime,
                ),
                (
                    kCGImagePropertyWebPDictionary,
                    kCGImagePropertyWebPUnclampedDelayTime,
                    kCGImagePropertyWebPDelayTime,
                ),
                (
                    kCGImagePropertyHEICSDictionary,
                    kCGImagePropertyHEICSUnclampedDelayTime,
                    kCGImagePropertyHEICSDelayTime,
                ),
            ]
            .into_iter()
            .find_map(|(dict, unclamped, clamped)| {
                let format = props.get(dict)?;
                let format: &CFDictionary<CFString, CFType> =
                    format.downcast_ref::<CFDictionary>()?.cast_unchecked();
                let number = |key| {
                    format
                        .get(key)
                        .and_then(|v| v.downcast_ref::<CFNumber>().and_then(|n| n.as_cgfloat()))
                        .filter(|s| *s > 0.0)
                };
                number(unclamped).or_else(|| number(clamped))
            })
        })
    };
    match seconds {
        Some(s) if s >= 0.02 => (s * 100.0).round() as u32,
        _ => 10,
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::canvas::render::ImageFormat;
    use objc2_core_foundation::{CFData, CFMutableData, CFRetained, CGPoint, CGRect, CGSize};
    use objc2_core_graphics::{CGBitmapContextCreateImage, CGContext, CGImage};
    use objc2_image_io::CGImageDestination;

    use super::*;
    use crate::canvas_image::{bitmap_context, encode};

    /// An opaque `side` × `side` image.
    fn image(side: usize) -> CFRetained<CGImage> {
        let context = bitmap_context(side, side).unwrap();
        let all = CGRect::new(CGPoint::ZERO, CGSize::new(side as f64, side as f64));
        CGContext::fill_rect(Some(&context), all);
        CGBitmapContextCreateImage(Some(&context)).unwrap()
    }

    /// An `side` × `side` PNG.
    fn png(side: usize) -> Vec<u8> {
        encode(&image(side), ImageFormat::Png).unwrap()
    }

    /// Images `sides` pixels square, in that order, in one file of type
    /// `uti` written by ImageIO (a GIF's frames, a TIFF's pages).
    fn written(uti: &str, sides: &[usize]) -> Vec<u8> {
        let out = CFMutableData::new(None, 0).unwrap();
        // SAFETY: a valid mutable data and type identifier, no options;
        // `sides.len()` images added before `finalize`.
        unsafe {
            let destination =
                CGImageDestination::with_data(&out, &CFString::from_str(uti), sides.len(), None)
                    .unwrap();
            for side in sides {
                destination.add_image(&image(*side), None);
            }
            assert!(destination.finalize());
        }
        out.to_vec()
    }

    /// An ICO of PNGs `sides` pixels square, in that order, as favicons
    /// are.
    fn ico(sides: &[usize]) -> Vec<u8> {
        let pngs: Vec<Vec<u8>> = sides.iter().map(|side| png(*side)).collect();
        let mut out = vec![0, 0, 1, 0, sides.len() as u8, 0];
        let mut offset = 6 + 16 * sides.len();
        for (side, png) in sides.iter().zip(&pngs) {
            let side = (*side % 256) as u8;
            out.extend([side, side, 0, 0, 1, 0, 32, 0]);
            out.extend((png.len() as u32).to_le_bytes());
            out.extend((offset as u32).to_le_bytes());
            offset += png.len();
        }
        for png in &pngs {
            out.extend(png);
        }
        out
    }

    fn source(bytes: &[u8]) -> CFRetained<CGImageSource> {
        // SAFETY: valid data.
        unsafe { CGImageSource::with_data(&CFData::from_bytes(bytes), None) }.unwrap()
    }

    /// The longest side of the one image `range` holds.
    fn side(source: &CGImageSource, range: Range<usize>) -> u32 {
        assert_eq!(range.len(), 1, "one image");
        let (w, h) = image_size(source, range.start);
        w.max(h)
    }

    #[test]
    fn a_multi_size_icon_shows_the_image_that_fits() {
        for sides in [[16, 32, 48], [48, 32, 16], [32, 16, 48]] {
            let icon = source(&ico(&sides));
            assert!(!is_animation(&icon), "{sides:?} is no animation");
            assert_eq!(side(&icon, shown(&icon, 24, 600)), 32);
            assert_eq!(side(&icon, shown(&icon, 32, 600)), 32);
            assert_eq!(side(&icon, shown(&icon, 12, 600)), 16);
            assert_eq!(
                side(&icon, shown(&icon, 64, 600)),
                48,
                "none fits: the largest"
            );
            assert_eq!(
                side(&icon, shown(&icon, 0, 600)),
                48,
                "natural size: the largest"
            );
            assert_eq!(side(&icon, shown(&icon, 24, 1)), 32);
            assert_eq!(source_size(&icon), (48, 48));
        }
    }

    #[test]
    fn a_multi_page_tiff_shows_one_page() {
        let tiff = source(&written("public.tiff", &[40, 20]));
        assert!(!is_animation(&tiff));
        assert_eq!(side(&tiff, shown(&tiff, 16, 600)), 20);
        assert_eq!(source_size(&tiff), (40, 40));
    }

    #[test]
    fn an_animation_plays_every_frame() {
        let gif = source(&written("com.compuserve.gif", &[8, 8, 8]));
        assert!(is_animation(&gif));
        assert_eq!(shown(&gif, 4, 600), 0..3);
        assert_eq!(shown(&gif, 4, 2), 0..2);
        assert_eq!(shown(&gif, 0, 1), 0..1, "its first frame alone");
        assert_eq!(frame_delay(&gif, 1), 10);
    }

    #[test]
    fn a_single_image_is_shown_whole() {
        let png = source(&png(8));
        assert!(!is_animation(&png));
        assert_eq!(shown(&png, 4, 600), 0..1);
        assert_eq!(source_size(&png), (8, 8));
    }
}

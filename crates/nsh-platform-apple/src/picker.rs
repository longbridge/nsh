//! Image preparation for Apple's photo pickers and modules supplying their own bytes.
use nsh_core::{
    ImageData,
    geometry::{DevicePixels, Size},
    picker::{Encoding, PickRequest, PickedFile},
};
use objc2_core_foundation::{
    CFData, CFDictionary, CFMutableData, CFNumber, CFString, CFType, CGFloat, kCFBooleanTrue,
};
use objc2_core_graphics::CGImage;

/// `bytes`, an image file named `name`, prepared as `request` says: kept,
/// or decoded with its orientation applied, scaled down and encoded again,
/// as `Picker.pickImages` prepares the photos picked (modules preparing
/// images of their own, a photo taken, call it too). `None` when it is no
/// image ImageIO reads. Runs on any thread.
pub fn prepare_image(bytes: Vec<u8>, name: &str, request: &PickRequest) -> Option<PickedFile> {
    use objc2_image_io::{
        CGImageDestination, CGImageSource, kCGImageDestinationLossyCompressionQuality,
        kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
        kCGImageSourceThumbnailMaxPixelSize,
    };
    let data = CFData::from_bytes(&bytes);
    // SAFETY: ImageIO's documented API, with option dictionaries of the
    // documented key and value types.
    unsafe {
        let source = CGImageSource::with_data(&data, None)?;
        let uti = source.r#type().map(|t| t.to_string()).unwrap_or_default();
        let mime = mime_for_uti(&uti)
            .or_else(|| nsh_core::picker::sniff(&bytes))
            .unwrap_or("application/octet-stream");
        let index = crate::image_source::shown(&source, 0, 1).start;
        let size = crate::image_source::image_size(&source, index);
        let size = Size::new(DevicePixels(size.0 as i32), DevicePixels(size.1 as i32));
        let encoding = request.needs_encoding(mime, size);
        if encoding == Encoding::Keep {
            let mut file = PickedFile::new(name, mime, ImageData::new(bytes));
            file.size = size;
            return Some(file);
        }
        let target = request.scaled_size(size);
        let longer = target.width.0.max(target.height.0).max(1);
        let yes: &CFType = kCFBooleanTrue?;
        let max = CFNumber::new_cgfloat(longer as CGFloat);
        let keys: [&CFString; 3] = [
            kCGImageSourceCreateThumbnailFromImageAlways,
            kCGImageSourceCreateThumbnailWithTransform,
            kCGImageSourceThumbnailMaxPixelSize,
        ];
        let values: [&CFType; 3] = [yes, yes, &max];
        let options = CFDictionary::from_slices(&keys, &values);
        let image = source.thumbnail_at_index(index, Some(options.as_opaque()))?;
        let (uti, mime) = match encoding {
            Encoding::Png => ("public.png", "image/png"),
            _ => ("public.jpeg", "image/jpeg"),
        };
        let out = CFMutableData::new(None, 0)?;
        let destination = CGImageDestination::with_data(&out, &CFString::from_str(uti), 1, None)?;
        let quality = CFNumber::new_cgfloat(request.jpeg_quality().0 as CGFloat);
        let keys: [&CFString; 1] = [kCGImageDestinationLossyCompressionQuality];
        let values: [&CFType; 1] = [&quality];
        let properties = CFDictionary::from_slices(&keys, &values);
        destination.add_image(&image, Some(properties.as_opaque()));
        if !destination.finalize() {
            return None;
        }
        let mut file = PickedFile::new(name, mime, ImageData::new(out.to_vec()));
        file.size = Size::new(
            DevicePixels(CGImage::width(Some(&image)) as i32),
            DevicePixels(CGImage::height(Some(&image)) as i32),
        );
        Some(file)
    }
}

/// The media type of an image's uniform type identifier.
fn mime_for_uti(uti: &str) -> Option<&'static str> {
    Some(match uti {
        "public.jpeg" => "image/jpeg",
        "public.png" => "image/png",
        "com.compuserve.gif" => "image/gif",
        "org.webmproject.webp" => "image/webp",
        "public.heic" => "image/heic",
        "public.heif" | "public.heics" => "image/heif",
        "public.avif" => "image/avif",
        "public.tiff" => "image/tiff",
        "com.microsoft.bmp" => "image/bmp",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_uses_the_primary_photo_of_a_heic_collection() {
        // ImageIO collection with a 32 × 32 first image and an 80 × 32 primary image.
        let collection = include_bytes!("../tests/fixtures/primary-second.heic");
        let mut request = PickRequest::default();
        request.max_dimension = Some(DevicePixels(20));
        let photo = prepare_image(collection.to_vec(), "collection.heic", &request).unwrap();
        assert_eq!(photo.mime, "image/jpeg");
        let size = nsh_core::picker::image_size(&photo.data).unwrap();
        assert_eq!((size.width.0, size.height.0), (20, 8));
        assert_eq!(photo.size, size);
    }
}

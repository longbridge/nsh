//! Premultiplied RGBA plus-darker, including partially transparent layers.

/// Saturated alpha addition followed by complement addition on premultiplied
/// color. Subtracting 255 from RGB unconditionally breaks transparent sources.
pub(crate) fn plus_darker(destination: &mut [u8], source: &[u8]) {
    debug_assert_eq!(destination.len(), source.len());
    for (destination, source) in destination
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(source.as_chunks::<4>().0)
    {
        let alpha = u16::from(destination[3]) + u16::from(source[3]);
        let excess = alpha.saturating_sub(255);
        for index in 0..3 {
            destination[index] = (u16::from(destination[index]) + u16::from(source[index]))
                .saturating_sub(excess)
                .min(255) as u8;
        }
        destination[3] = alpha.min(255) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opaque_complements_and_transparent_identity() {
        let mut pixels = [204, 153, 51, 255, 204, 153, 51, 255, 0, 0, 0, 0];
        plus_darker(
            &mut pixels,
            &[102, 128, 230, 255, 0, 0, 0, 0, 102, 128, 230, 255],
        );
        assert_eq!(
            pixels,
            [51, 26, 26, 255, 204, 153, 51, 255, 102, 128, 230, 255]
        );
    }
    #[test]
    fn alpha_addition_matches_core_graphics_reference() {
        // RGBA8 sRGB CGContext, destination=(.8,.6,.2), source=(.4,.5,.9).
        // Both paints have alpha=.5; Core Graphics differs by at most one
        // rounding unit from these already quantized premultiplied samples.
        let mut destination = [102, 77, 26, 128];
        plus_darker(&mut destination, &[51, 64, 115, 128]);
        assert_eq!(destination, [152, 140, 140, 255]);
        let mut destination = [51, 38, 13, 64];
        plus_darker(&mut destination, &[26, 32, 57, 64]);
        assert_eq!(destination, [77, 70, 70, 128]);
    }
}

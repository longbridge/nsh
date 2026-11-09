//! The test pattern every development host registers as the live stream
//! `nsh/test-pattern` (`<Video srcObject={HostStream.fromHost(…)}>`), so
//! an app's tests can play a stream on every target (`nsh test --target`)
//! without a host of their own: a color sweeping through the hues behind
//! a bar crossing left to right, so a still and a stuck stream look
//! different. The frames are BGRA, [`WIDTH`] × [`HEIGHT`] pixels; each
//! platform's host wraps them in its own frame type. The name is under
//! `nsh/`, which is NSH's own: the hosts register it through the core's
//! `Streams::register_reserved` (the embedding app's `register` refuses
//! it), and apps playing it grant it in `capabilities.streams`.

use std::{thread, time::Duration};

/// The stream's name, the one apps play (`HostStream.fromHost(NAME)`).
pub const NAME: &str = "nsh/test-pattern";

/// The frames' width (pixels).
pub const WIDTH: usize = 320;

/// The frames' height (pixels).
pub const HEIGHT: usize = 180;

/// A row's bytes: the frames are packed BGRA.
pub const STRIDE: usize = WIDTH * 4;

/// The time between frames (about 30 a second).
pub const FRAME_MS: u64 = 33;

/// Starts a thread pushing a frame to `push` every [`FRAME_MS`], for as
/// long as the process runs: `WIDTH * HEIGHT * 4` bytes of packed BGRA.
pub fn start(push: impl Fn(&[u8]) + Send + 'static) {
    thread::spawn(move || {
        let mut bgra = vec![0u8; STRIDE * HEIGHT];
        let mut tick: u32 = 0;
        loop {
            frame(tick, &mut bgra);
            push(&bgra);
            tick = tick.wrapping_add(1);
            thread::sleep(Duration::from_millis(FRAME_MS));
        }
    });
}

/// Draws the pattern at `tick` into `bgra` (`STRIDE * HEIGHT` bytes).
pub fn frame(tick: u32, bgra: &mut [u8]) {
    assert!(
        bgra.len() >= STRIDE * HEIGHT,
        "a frame is {} bytes",
        STRIDE * HEIGHT
    );
    let (r, g, b) = hue((tick % 360) as f32);
    let bar = (tick as usize * 4) % WIDTH;
    for y in 0..HEIGHT {
        let row = &mut bgra[y * STRIDE..y * STRIDE + WIDTH * 4];
        for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            *px = if x.abs_diff(bar) < 8 {
                [255, 255, 255, 255]
            } else {
                [b, g, r, 255]
            };
        }
    }
}

/// A saturated color at `degrees` of hue, as (red, green, blue).
fn hue(degrees: f32) -> (u8, u8, u8) {
    let h = degrees / 60.0;
    let x = (1.0 - (h % 2.0 - 1.0).abs()) * 255.0;
    let (r, g, b) = match h as u32 {
        0 => (255.0, x, 0.0),
        1 => (x, 255.0, 0.0),
        2 => (0.0, 255.0, x),
        3 => (0.0, x, 255.0),
        4 => (x, 0.0, 255.0),
        _ => (255.0, 0.0, x),
    };
    (r as u8, g as u8, b as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_change_with_the_tick() {
        let mut a = vec![0u8; STRIDE * HEIGHT];
        let mut b = vec![0u8; STRIDE * HEIGHT];
        frame(0, &mut a);
        frame(40, &mut b);
        assert_ne!(a, b);
        // Opaque, with the bar white at its start.
        assert!(a.as_chunks::<4>().0.iter().all(|px| px[3] == 255));
        assert_eq!(&a[0..4], &[255, 255, 255, 255]);
        assert_eq!(&a[STRIDE / 2..STRIDE / 2 + 4], &[0, 0, 255, 255]);
    }
}

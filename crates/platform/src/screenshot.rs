//! Screen capture for the "take screenshot" hotkey.
//!
//! This captures what Windows is currently showing on a monitor. It works for
//! windowed and borderless games. Exclusive-fullscreen games bypass the desktop
//! compositor, so the capture comes back blank – that is detected and reported
//! rather than saving a black picture.

pub struct Shot {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA.
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub enum ShotError {
    Unsupported,
    Failed(String),
    /// The result was entirely black (typical for exclusive full-screen).
    Blank,
}

impl std::fmt::Display for ShotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShotError::Unsupported => write!(f, "Screenshots are only available on Windows."),
            ShotError::Failed(e) => write!(f, "The screenshot could not be taken: {e}"),
            ShotError::Blank => write!(
                f,
                "The screenshot came out black. The game is probably in exclusive full-screen mode, which Windows does not let other apps capture. Switch the game to borderless or windowed mode and try again."
            ),
        }
    }
}

/// Capture the monitor named `monitor` (e.g. `\\.\DISPLAY1`), or the primary monitor.
pub fn capture_monitor(monitor: Option<&str>) -> Result<Shot, ShotError> {
    #[cfg(windows)]
    {
        let s = crate::windows::screenshot::capture(monitor)?;
        if looks_blank(&s) {
            return Err(ShotError::Blank);
        }
        Ok(s)
    }
    #[cfg(not(windows))]
    {
        let _ = monitor;
        Err(ShotError::Unsupported)
    }
}

/// True when (almost) every sampled pixel is black.
pub fn looks_blank(s: &Shot) -> bool {
    if s.rgba.len() < 4 {
        return true;
    }
    let px = s.rgba.len() / 4;
    let step = (px / 4096).max(1);
    let mut lit = 0usize;
    let mut n = 0usize;
    let mut i = 0usize;
    while i < px {
        let o = i * 4;
        if s.rgba[o] > 6 || s.rgba[o + 1] > 6 || s.rgba[o + 2] > 6 {
            lit += 1;
        }
        n += 1;
        i += step;
    }
    lit * 200 < n // fewer than 0.5 % of samples have any light
}

/// Box-filter downscale to at most `max_w` pixels wide (used for gallery thumbnails).
pub fn thumbnail(s: &Shot, max_w: u32) -> Shot {
    if s.width <= max_w || max_w == 0 || s.width == 0 || s.height == 0 {
        return Shot { width: s.width, height: s.height, rgba: s.rgba.clone() };
    }
    let f = s.width as f64 / max_w as f64;
    let nw = max_w;
    let nh = ((s.height as f64 / f).round() as u32).max(1);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        let y0 = (y as f64 * f) as u32;
        let y1 = (((y + 1) as f64 * f) as u32).clamp(y0 + 1, s.height);
        for x in 0..nw {
            let x0 = (x as f64 * f) as u32;
            let x1 = (((x + 1) as f64 * f) as u32).clamp(x0 + 1, s.width);
            let mut acc = [0u32; 3];
            let mut n = 0u32;
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let o = ((yy * s.width + xx) * 4) as usize;
                    acc[0] += s.rgba[o] as u32;
                    acc[1] += s.rgba[o + 1] as u32;
                    acc[2] += s.rgba[o + 2] as u32;
                    n += 1;
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            out[o] = (acc[0] / n) as u8;
            out[o + 1] = (acc[1] / n) as u8;
            out[o + 2] = (acc[2] / n) as u8;
            out[o + 3] = 255;
        }
    }
    Shot { width: nw, height: nh, rgba: out }
}

pub fn encode_png(s: &Shot) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, s.width, s.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&s.rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_frames_are_detected() {
        let black = Shot { width: 64, height: 64, rgba: vec![0; 64 * 64 * 4] };
        assert!(looks_blank(&black));
        let mut lit = Shot { width: 64, height: 64, rgba: vec![0; 64 * 64 * 4] };
        for p in lit.rgba.chunks_mut(4).take(2000) {
            p.copy_from_slice(&[200, 120, 40, 255]);
        }
        assert!(!looks_blank(&lit));
    }

    #[test]
    fn thumbnails_keep_aspect_ratio_and_average_colours() {
        let mut big = Shot { width: 400, height: 200, rgba: vec![0; 400 * 200 * 4] };
        for p in big.rgba.chunks_mut(4) {
            p.copy_from_slice(&[100, 150, 200, 255]);
        }
        let t = thumbnail(&big, 100);
        assert_eq!((t.width, t.height), (100, 50));
        assert_eq!(&t.rgba[0..4], &[100, 150, 200, 255]);
        let same = thumbnail(&big, 800);
        assert_eq!(same.width, 400);
    }

    #[test]
    fn png_encoding_produces_a_valid_png() {
        let s = Shot { width: 4, height: 4, rgba: vec![255; 4 * 4 * 4] };
        let bytes = encode_png(&s).unwrap();
        assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    }
}

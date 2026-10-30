//! HDR image buffer and file output (PNG: tone-mapped 8-bit sRGB; EXR: linear HDR, `docs/PRD.md` FR-X1/FR-X2).

use std::path::Path;

use image::{ImageBuffer, Rgb};

use crate::RenderError;
use crate::tonemap::{ToneMap, to_srgb8};

/// Linear sRGB (Rec. 709 primaries, D65) image, row-major from the top-left, in the units of the band LUT.
#[derive(Clone, Debug, PartialEq)]
pub struct HdrImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height` RGB triples.
    pub pixels: Vec<[f32; 3]>,
}

impl HdrImage {
    /// Pixel `(i, j)` (column, row).
    pub fn pixel(&self, i: u32, j: u32) -> [f32; 3] {
        self.pixels[(j * self.width + i) as usize]
    }
}

/// Output file format, from the file extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// Tone-mapped 8-bit sRGB PNG.
    Png,
    /// Linear (exposed, not tone-mapped) 32-bit float OpenEXR.
    Exr,
}

impl OutputFormat {
    /// Format for `path` (`.png` or `.exr`, case-insensitive).
    ///
    /// # Errors
    /// [`RenderError::OutputFormat`] for other extensions.
    pub fn from_path(path: &Path) -> Result<Self, RenderError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        match ext.as_str() {
            "png" => Ok(Self::Png),
            "exr" => Ok(Self::Exr),
            _ => Err(RenderError::OutputFormat(ext)),
        }
    }
}

/// Writes `image` to `path` (format from the extension; parent directories are created). PNG applies the
/// exposure `scale` and the tone map; EXR stores `scale · rgb` unclamped, so viewers show the same exposure.
///
/// # Errors
/// I/O or encoding errors, or an unsupported extension.
pub fn write_image(path: &Path, image: &HdrImage, scale: f64, tone_map: ToneMap) -> Result<(), RenderError> {
    let format = OutputFormat::from_path(path)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let (w, h) = (image.width, image.height);
    match format {
        OutputFormat::Png => {
            let bytes = to_srgb8(&image.pixels, scale, tone_map);
            let buf =
                ImageBuffer::<Rgb<u8>, _>::from_raw(w, h, bytes).expect("buffer size matches the image");
            buf.save_with_format(path, image::ImageFormat::Png)?;
        }
        OutputFormat::Exr => {
            let data: Vec<f32> = image
                .pixels
                .iter()
                .flat_map(|p| p.map(|c| (f64::from(c) * scale) as f32))
                .collect();
            let buf =
                ImageBuffer::<Rgb<f32>, _>::from_raw(w, h, data).expect("buffer size matches the image");
            buf.save_with_format(path, image::ImageFormat::OpenExr)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_and_exr_round_trip() {
        let dir = std::env::temp_dir().join(format!("gravography-output-test-{}", std::process::id()));
        let img = HdrImage {
            width: 3,
            height: 2,
            pixels: vec![
                [0.0, 0.5, 2.0],
                [1.0; 3],
                [0.25; 3],
                [8.0, 0.0, 0.0],
                [0.0; 3],
                [100.0; 3],
            ],
        };
        let exr = dir.join("t.exr");
        write_image(&exr, &img, 0.5, ToneMap::Aces).unwrap();
        let back = image::open(&exr).unwrap().into_rgb32f();
        assert_eq!(back.dimensions(), (3, 2));
        // EXR stores f32 exactly: values are scale · rgb (0.5 is exact in binary).
        assert_eq!(back.get_pixel(0, 0).0, [0.0, 0.25, 1.0]);
        assert_eq!(back.get_pixel(2, 1).0, [50.0; 3]);
        let png = dir.join("sub/t.png");
        write_image(&png, &img, 1.0, ToneMap::Aces).unwrap();
        let back = image::open(&png).unwrap().into_rgb8();
        assert_eq!(back.get_pixel(1, 1).0, [0, 0, 0]);
        assert_eq!(back.get_pixel(2, 1).0, [255, 255, 255]);
        assert_eq!(
            &to_srgb8(&img.pixels, 1.0, ToneMap::Aces)[..3],
            &back.get_pixel(0, 0).0
        );
        assert!(matches!(
            write_image(&dir.join("t.jpg"), &img, 1.0, ToneMap::Aces),
            Err(RenderError::OutputFormat(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

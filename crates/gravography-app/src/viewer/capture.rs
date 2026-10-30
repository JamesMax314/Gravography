//! Window-frame capture: copies the presented surface texture (3D view, panel and HUD exactly as they appear in
//! the window) to a PNG. Used by `--smoke-test` so the agent can inspect the composed window without OS-level
//! screen capture.

use std::path::Path;

use anyhow::{Context, Result, bail};
use gravography_render::gpu::GpuContext;
use gravography_render::output::write_rgb8_png;

/// Row pitch of a texture-to-buffer copy: `width × 4` bytes rounded up to `COPY_BYTES_PER_ROW_ALIGNMENT`.
pub fn padded_row_bytes(width: u32) -> u32 {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    (width * 4).div_ceil(align) * align
}

/// Converts padded 4-byte-per-pixel rows to tightly packed RGB. `bgra` swaps the red and blue channels.
pub fn unpad_to_rgb(data: &[u8], width: u32, height: u32, row_bytes: u32, bgra: bool) -> Vec<u8> {
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for row in data.chunks(row_bytes as usize).take(height as usize) {
        for px in row[..(width * 4) as usize].as_chunks::<4>().0 {
            if bgra {
                rgb.extend_from_slice(&[px[2], px[1], px[0]]);
            } else {
                rgb.extend_from_slice(&px[..3]);
            }
        }
    }
    rgb
}

/// Copies `texture` (a surface texture created with `COPY_SRC`) to `path` as an RGB PNG. 8-bit sRGB-encoded
/// formats are stored as they are (the bytes already hold display values).
///
/// # Errors
/// Unsupported format, GPU readback or file errors.
pub fn capture_texture(ctx: &GpuContext, texture: &wgpu::Texture, path: &Path) -> Result<()> {
    let (w, h) = (texture.width(), texture.height());
    let bgra = match texture.format() {
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => true,
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => false,
        f => bail!("window capture does not support the surface format {f:?}"),
    };
    let row = padded_row_bytes(w);
    let buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("window capture"),
        size: u64::from(row) * u64::from(h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("window capture"),
        });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    ctx.queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    ctx.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| anyhow::anyhow!("poll failed: {e}"))?;
    rx.recv()
        .context("map callback dropped")?
        .context("mapping the capture buffer")?;
    let rgb = {
        let view = slice
            .get_mapped_range()
            .map_err(|e| anyhow::anyhow!("mapped range: {e}"))?;
        unpad_to_rgb(&view, w, h, row, bgra)
    };
    write_rgb8_png(path, w, h, rgb)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_aligned_and_unpadded() {
        assert_eq!(padded_row_bytes(64), 256);
        assert_eq!(padded_row_bytes(65), 512);
        // 2×2 BGRA image in 256-byte rows: pixel (x, y) = [b, g, r, a] = [x, y, 9, 255].
        let mut data = vec![0u8; 512];
        for y in 0..2u8 {
            for x in 0..2u8 {
                let i = usize::from(y) * 256 + usize::from(x) * 4;
                data[i..i + 4].copy_from_slice(&[x, y, 9, 255]);
            }
        }
        assert_eq!(
            unpad_to_rgb(&data, 2, 2, 256, true),
            [9, 0, 0, 9, 0, 1, 9, 1, 0, 9, 1, 1]
        );
        assert_eq!(unpad_to_rgb(&data, 2, 2, 256, false)[..3], [0, 0, 9]);
    }
}

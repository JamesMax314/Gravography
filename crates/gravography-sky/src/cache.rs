//! Optional on-disk cache of baked skies (`docs/PRD.md` NF-6). The caller chooses the directory (the app uses the
//! platform cache dir). Files are keyed by a stable hash of [`SkyParams`]; a mismatching or corrupt file is
//! treated as a cache miss.
//!
//! Format (little endian): magic `GRVSKY01`, `u64` key, `u32` base face size, `u32` level count, then for each
//! level all texels as `f32` pairs in storage order.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::bake::{SkyParams, bake_sky};
use crate::cubemap::{CubeLevel, CubeMap};
use crate::{BlackbodyLut, SkyError};

const MAGIC: &[u8; 8] = b"GRVSKY01";
/// Bump when the bake algorithm changes, to invalidate old caches.
const BAKE_VERSION: &str = "bake-v1";

/// Stable 64-bit key of the parameters: FNV-1a of the bake version and the `Debug` text of `params` (Rust's float
/// formatting is shortest-round-trip, so equal parameters give equal text).
pub fn params_key(params: &SkyParams) -> u64 {
    let text = format!("{BAKE_VERSION}{params:?}");
    text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Cache file path for `params` inside `dir`.
pub fn cache_path(dir: &Path, params: &SkyParams) -> PathBuf {
    dir.join(format!("sky-{:016x}.bin", params_key(params)))
}

/// Writes `cube` to `path` with `key`.
///
/// # Errors
/// [`SkyError::Io`] on write failure.
pub fn save(path: &Path, key: u64, cube: &CubeMap) -> Result<(), SkyError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = BufWriter::new(fs::File::create(path)?);
    f.write_all(MAGIC)?;
    f.write_all(&key.to_le_bytes())?;
    let size =
        u32::try_from(cube.base_size()).map_err(|_| SkyError::InvalidParameters("face size too large"))?;
    let levels =
        u32::try_from(cube.levels.len()).map_err(|_| SkyError::InvalidParameters("too many levels"))?;
    f.write_all(&size.to_le_bytes())?;
    f.write_all(&levels.to_le_bytes())?;
    for l in &cube.levels {
        for t in &l.data {
            f.write_all(&t[0].to_le_bytes())?;
            f.write_all(&t[1].to_le_bytes())?;
        }
    }
    f.flush()?;
    Ok(())
}

/// Reads a cube map from `path` if it exists and was written with `key`; `Ok(None)` for a missing, mismatching or
/// malformed file.
///
/// # Errors
/// [`SkyError::Io`] for I/O errors other than "not found".
pub fn load(path: &Path, key: u64) -> Result<Option<CubeMap>, SkyError> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    Ok(parse(&bytes, key))
}

fn parse(bytes: &[u8], key: u64) -> Option<CubeMap> {
    let u32_at = |o: usize| {
        bytes
            .get(o..o + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };
    if bytes.get(..8)? != MAGIC {
        return None;
    }
    let stored = u64::from_le_bytes(bytes.get(8..16)?.try_into().ok()?);
    if stored != key {
        return None;
    }
    let size = u32_at(16)? as usize;
    let count = u32_at(20)? as usize;
    if !size.is_power_of_two() || count != size.trailing_zeros() as usize + 1 {
        return None;
    }
    let mut offset = 24;
    let mut levels = Vec::with_capacity(count);
    let mut s = size;
    for _ in 0..count {
        let mut level = CubeLevel::zeros(s);
        let len = level.data.len() * 8;
        let chunk = bytes.get(offset..offset + len)?;
        let (texels, _) = chunk.as_chunks::<8>();
        for (t, b) in level.data.iter_mut().zip(texels) {
            *t = [
                f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                f32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            ];
        }
        offset += len;
        levels.push(level);
        s /= 2;
    }
    (offset == bytes.len()).then_some(CubeMap { levels })
}

/// Bakes the sky, reading from and writing to `cache_dir` when given. A failed cache write is ignored (the bake
/// result is still returned); the boolean is `true` on a cache hit.
///
/// # Errors
/// Bake errors, or I/O errors while reading an existing cache file.
pub fn bake_sky_cached(
    params: &SkyParams,
    lut: &BlackbodyLut,
    cache_dir: Option<&Path>,
) -> Result<(CubeMap, bool), SkyError> {
    let key = params_key(params);
    if let Some(dir) = cache_dir
        && let Some(cube) = load(&cache_path(dir, params), key)?
    {
        return Ok((cube, true));
    }
    let (_, cube) = bake_sky(params, lut)?;
    if let Some(dir) = cache_dir {
        let _ = save(&cache_path(dir, params), key, &cube);
    }
    Ok((cube, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::StarParams;

    fn params() -> SkyParams {
        SkyParams {
            face_size: 8,
            stars: StarParams {
                count: 300,
                ..StarParams::default()
            },
            ..SkyParams::default()
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gravography-sky-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn round_trip_and_hit() {
        let lut = BlackbodyLut::human_vision();
        let dir = temp_dir("rt");
        let (a, hit) = bake_sky_cached(&params(), &lut, Some(&dir)).unwrap();
        assert!(!hit);
        let (b, hit) = bake_sky_cached(&params(), &lut, Some(&dir)).unwrap();
        assert!(hit);
        assert_eq!(a, b);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn key_depends_on_params_and_mismatch_is_a_miss() {
        let p = params();
        let q = SkyParams {
            stars: StarParams {
                seed: 99,
                ..p.stars.clone()
            },
            ..p.clone()
        };
        assert_eq!(params_key(&p), params_key(&params()));
        assert_ne!(params_key(&p), params_key(&q));
        let dir = temp_dir("miss");
        let lut = BlackbodyLut::human_vision();
        let (cube, _) = bake_sky_cached(&p, &lut, None).unwrap();
        let path = dir.join("x.bin");
        save(&path, params_key(&p), &cube).unwrap();
        assert!(load(&path, params_key(&q)).unwrap().is_none());
        assert_eq!(load(&path, params_key(&p)).unwrap(), Some(cube));
        // Truncated file → miss.
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, &bytes[..bytes.len() - 4]).unwrap();
        assert!(load(&path, params_key(&p)).unwrap().is_none());
        assert!(load(&dir.join("absent.bin"), 0).unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}

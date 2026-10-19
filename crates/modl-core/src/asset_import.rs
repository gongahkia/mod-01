//! Bounded PNG admission and one deterministic master-palette converter for all Rust tools.
use crate::{CartridgeError, hardware::master_palette};
use image::{ImageFormat, ImageReader, Limits};
use serde_json::{Value, json};
use std::io::Cursor;

const MAX_PNG_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PngAssetKind {
    Sprite,
    TileSet,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbaSource {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

fn error(message: impl Into<String>) -> CartridgeError {
    CartridgeError {
        code: "M014065",
        message: message.into(),
    }
}

/// Alpha 0–127 maps to zero; opaque colours choose the nearest RGB among indices 1–255.
/// Squared-RGB distance and lowest-index tie breaking are fixed, with no dithering.
#[must_use]
pub fn palette_index(pixel: [u8; 4]) -> u8 {
    if pixel[3] <= 127 {
        return 0;
    }
    let mut best = 1;
    let mut distance = i32::MAX;
    let palette = master_palette();
    for index in 1..=u8::MAX {
        let color = palette[usize::from(index)];
        let candidate = (0..3)
            .map(|channel| (i32::from(pixel[channel]) - i32::from(color[channel])).pow(2))
            .sum::<i32>();
        if candidate < distance {
            best = index;
            distance = candidate;
        }
    }
    best
}

fn dimensions(bytes: &[u8], kind: PngAssetKind) -> Result<(u32, u32), CartridgeError> {
    if bytes.len() > MAX_PNG_BYTES {
        return Err(error("PNG source exceeds 16 MiB"));
    }
    if bytes.len() < 33
        || bytes[..8] != [137, 80, 78, 71, 13, 10, 26, 10]
        || bytes[8..12] != [0, 0, 0, 13]
        || &bytes[12..16] != b"IHDR"
    {
        return Err(error("expected a PNG with a canonical IHDR header"));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("checked four-byte width"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("checked four-byte height"));
    let valid = match kind {
        PngAssetKind::Sprite => (1..=128).contains(&width) && (1..=128).contains(&height),
        PngAssetKind::TileSet => {
            width > 0
                && height > 0
                && width.is_multiple_of(8)
                && height.is_multiple_of(8)
                && (1..=4096).contains(&(u64::from(width / 8) * u64::from(height / 8)))
        }
    };
    if !valid {
        return Err(error(match kind {
            PngAssetKind::Sprite => "sprite PNG must be 1×1 through 128×128",
            PngAssetKind::TileSet => "tile PNG must contain 1–4096 complete 8×8 tiles",
        }));
    }
    Ok((width, height))
}

/// Checks source length and IHDR dimensions before constructing any pixel decoder.
/// The PNG decoder's encoding checks and dimension/allocation limits apply before admission.
/// # Errors
/// Returns M014065 for invalid, oversized, truncated or non-decodable PNG input.
pub fn decode_png(bytes: &[u8], kind: PngAssetKind) -> Result<RgbaSource, CartridgeError> {
    let (width, height) = dimensions(bytes, kind)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|failure| error(format!("PNG decode failed: {failure}")))?
        .into_rgba8();
    if decoded.dimensions() != (width, height) {
        return Err(error("PNG dimensions changed during decoding"));
    }
    Ok(RgbaSource {
        width: usize::try_from(width).map_err(|_| error("PNG width exceeds this host"))?,
        height: usize::try_from(height).map_err(|_| error("PNG height exceeds this host"))?,
        pixels: decoded.into_raw(),
    })
}

/// Converts an already admitted RGBA source into canonical current indexed asset JSON.
/// Existing optional metadata and tile flags survive replacement; colour zero is never selected
/// for an opaque pixel. This function never changes the supplied source or prior document.
/// # Errors
/// Returns M014065 for invalid RGBA dimensions/data, purpose, or a mismatched prior asset kind.
pub fn indexed_asset(
    source: &RgbaSource,
    kind: PngAssetKind,
    previous: Option<&Value>,
) -> Result<Value, CartridgeError> {
    let valid_size = match kind {
        PngAssetKind::Sprite => {
            (1..=128).contains(&source.width) && (1..=128).contains(&source.height)
        }
        PngAssetKind::TileSet => {
            source.width > 0
                && source.height > 0
                && source.width.is_multiple_of(8)
                && source.height.is_multiple_of(8)
                && source
                    .width
                    .checked_div(8)
                    .and_then(|width| width.checked_mul(source.height / 8))
                    .is_some_and(|count| (1..=4096).contains(&count))
        }
    };
    if !valid_size
        || source
            .width
            .checked_mul(source.height)
            .and_then(|count| count.checked_mul(4))
            != Some(source.pixels.len())
    {
        return Err(error("RGBA buffer/dimensions are invalid for this import"));
    }
    let name = match kind {
        PngAssetKind::Sprite => "sprite",
        PngAssetKind::TileSet => "tile_set",
    };
    if previous.is_some_and(|value| !value.is_object() || value["kind"] != name) {
        return Err(error(format!(
            "cannot replace a different asset kind with {name}"
        )));
    }
    let mut value = previous.cloned().unwrap_or_else(|| json!({}));
    value["kind"] = json!(name);
    value["revision"] = json!(2);
    let indices = source
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(palette_index)
        .collect::<Vec<_>>();
    match kind {
        PngAssetKind::Sprite => {
            value["width"] = json!(source.width);
            value["height"] = json!(source.height);
            value["frames"] = json!([indices]);
        }
        PngAssetKind::TileSet => {
            let columns = source.width / 8;
            let rows = source.height / 8;
            let count = columns * rows;
            let tiles = (0..count)
                .map(|tile| {
                    (0..64)
                        .map(|pixel| {
                            indices[(tile / columns * 8 + pixel / 8) * source.width
                                + tile % columns * 8
                                + pixel % 8]
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let mut flags = value["flags"].as_array().cloned().unwrap_or_default();
            flags.resize(count, json!(0));
            flags.truncate(count);
            value["tiles"] = json!(tiles);
            value["flags"] = json!(flags);
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, pixels: Vec<u8>) -> Vec<u8> {
        let image = image::RgbaImage::from_raw(width, height, pixels).unwrap();
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn all_opaque_palette_entries_and_alpha_thresholds_have_one_deterministic_quantizer() {
        for index in 1..=u8::MAX {
            let [r, g, b] = master_palette()[usize::from(index)];
            assert_eq!(palette_index([r, g, b, 128]), index);
            assert_eq!(palette_index([r, g, b, 127]), 0);
        }
        assert_ne!(palette_index([23, 20, 31, 255]), 0);
        let rgb = [119, 37, 92, 255];
        let reference = (1_u16..=255)
            .min_by_key(|index| {
                let color = master_palette()[usize::from(*index)];
                (0..3)
                    .map(|channel| (i32::from(rgb[channel]) - i32::from(color[channel])).pow(2))
                    .sum::<i32>()
            })
            .unwrap();
        assert_eq!(u16::from(palette_index(rgb)), reference);
    }

    #[test]
    fn decoder_rejects_bad_or_huge_headers_before_allocating_pixels() {
        for bytes in [&[][..], b"not a png", &[0; 33]] {
            assert!(decode_png(bytes, PngAssetKind::Sprite).is_err());
        }
        let mut image = png(1, 1, vec![255; 4]);
        image[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        image[20..24].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(
            decode_png(&image, PngAssetKind::Sprite)
                .unwrap_err()
                .message
                .contains("128")
        );
        assert!(
            decode_png(&image, PngAssetKind::TileSet)
                .unwrap_err()
                .message
                .contains("4096")
        );
        let truncated = png(1, 1, vec![255; 4]);
        assert!(decode_png(&truncated[..33], PngAssetKind::Sprite).is_err());
    }

    #[test]
    fn sprite_and_tile_imports_preserve_source_colours_metadata_flags_and_row_major_tile_order() {
        let mut pixels = Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                let color = [7_u8, 9, 11, 25][y / 8 * 2 + x / 8];
                let [r, g, b] = master_palette()[usize::from(color)];
                pixels.extend_from_slice(&[r, g, b, if x == 0 && y == 0 { 127 } else { 255 }]);
            }
        }
        let source = decode_png(&png(16, 16, pixels.clone()), PngAssetKind::TileSet).unwrap();
        assert_eq!(source.pixels, pixels);
        let previous =
            json!({"revision":2,"kind":"tile_set","tiles":[],"flags":[1,2],"memo":"retain"});
        let converted = indexed_asset(&source, PngAssetKind::TileSet, Some(&previous)).unwrap();
        assert_eq!(converted["flags"], json!([1, 2, 0, 0]));
        assert_eq!(converted["memo"], "retain");
        assert_eq!(converted["tiles"][0][0], 0);
        for (tile, color) in [7, 9, 11, 25].iter().enumerate() {
            assert_eq!(converted["tiles"][tile][63], *color);
        }
        let sprite = indexed_asset(&source, PngAssetKind::Sprite, None).unwrap();
        assert_eq!(sprite["frames"][0].as_array().unwrap().len(), 256);
        assert_eq!(source.pixels, pixels);
        assert_eq!(previous["flags"], json!([1, 2]));
        assert!(indexed_asset(&source, PngAssetKind::Sprite, Some(&previous)).is_err());
    }
}

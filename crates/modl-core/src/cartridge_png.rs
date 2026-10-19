use serde::{Deserialize, Serialize};

use crate::game::{MAX_GAME_PACKED_BYTES, PackedGame, decode_game};

const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
/// One bounded shipping game plus PNG image/chunk overhead. Logical ROMs retain
/// their separate 1 MiB admission limit; this is not a second machine profile.
pub const MAX_CARTRIDGE_PNG_BYTES: usize = MAX_GAME_PACKED_BYTES + 8 * 1024 * 1024;
const WIDTH: usize = 320;
const HEIGHT: usize = 240;
const PALETTE: [[u8; 3]; 8] = [
    [23, 20, 31],
    [41, 37, 50],
    [64, 57, 70],
    [93, 80, 84],
    [128, 106, 99],
    [170, 139, 116],
    [213, 185, 146],
    [244, 229, 189],
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CartridgePngMetadata {
    pub title: String,
    pub author: String,
    pub year: u16,
    pub players: u8,
    pub controls: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CartridgePng {
    pub cartridge: Vec<u8>,
    pub metadata: CartridgePngMetadata,
}

/// Uses the admitted main cartridge's presentation metadata, regardless of which
/// member an editor is displaying. Native Studio and CLI export share this path.
///
/// # Errors
/// Returns a current source, metadata or game-admission error before emitting PNG.
pub fn encode_game_png(game: &PackedGame) -> Result<Vec<u8>, String> {
    let project = crate::parse_workcart(game.source()).map_err(|error| error.to_string())?;
    let identity = project
        .files
        .get("presentation/cartridge.json")
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok());
    let metadata = CartridgePngMetadata {
        title: game.entry().manifest.title.clone(),
        author: game.entry().manifest.author.clone(),
        year: identity
            .as_ref()
            .and_then(|value| value["year"].as_u64())
            .and_then(|value| u16::try_from(value).ok())
            .filter(|year| (1970..=9999).contains(year))
            .unwrap_or(1999),
        players: identity
            .as_ref()
            .and_then(|value| value["players"].as_u64())
            .and_then(|value| u8::try_from(value).ok())
            .filter(|players| (1..=4).contains(players))
            .unwrap_or(1),
        controls: identity
            .as_ref()
            .and_then(|value| value["controls"].as_str())
            .filter(|value| value.len() <= 64)
            .unwrap_or("PAD")
            .to_owned(),
    };
    encode_cartridge_png(game.bytes(), &metadata)
}

/// Encodes canonical cartridge bytes into the deterministic MOD-01 PNG object.
///
/// # Errors
///
/// Returns an error when the cartridge or identity metadata violates its bounded schema.
pub fn encode_cartridge_png(
    cartridge: &[u8],
    metadata: &CartridgePngMetadata,
) -> Result<Vec<u8>, String> {
    validate(cartridge, metadata)?;
    let mut rgba = vec![0_u8; WIDTH * HEIGHT * 4];
    fill(&mut rgba, 0, 0, WIDTH, HEIGHT, 2);
    fill(&mut rgba, 12, 8, 296, 224, 4);
    fill(&mut rgba, 20, 18, 280, 186, 1);
    fill(&mut rgba, 28, 26, 264, 162, 0);
    fill(&mut rgba, 20, 194, 280, 30, 7);
    let mut header = [0_u8; 13];
    header[..4].copy_from_slice(
        &u32::try_from(WIDTH)
            .map_err(|_| "width overflow")?
            .to_be_bytes(),
    );
    header[4..8].copy_from_slice(
        &u32::try_from(HEIGHT)
            .map_err(|_| "height overflow")?
            .to_be_bytes(),
    );
    header[8..].copy_from_slice(&[8, 6, 0, 0, 0]);
    let metadata = serde_json::to_vec(metadata).map_err(|error| error.to_string())?;
    let mut scanlines = vec![0_u8; HEIGHT * (1 + WIDTH * 4)];
    for row in 0..HEIGHT {
        let source = row * WIDTH * 4;
        let target = row * (1 + WIDTH * 4) + 1;
        scanlines[target..target + WIDTH * 4].copy_from_slice(&rgba[source..source + WIDTH * 4]);
    }
    let mut output = PNG_SIGNATURE.to_vec();
    chunk(&mut output, *b"IHDR", &header)?;
    chunk(&mut output, *b"moCm", &metadata)?;
    chunk(&mut output, *b"moCa", cartridge)?;
    chunk(&mut output, *b"IDAT", &zlib_stored(&scanlines)?)?;
    chunk(&mut output, *b"IEND", &[])?;
    Ok(output)
}

/// Extracts one CRC-checked bounded cartridge and its identity metadata from PNG.
///
/// # Errors
///
/// Returns an error for malformed chunks, bad CRCs, duplicates, trailing bytes, or invalid bounds.
pub fn decode_cartridge_png(bytes: &[u8]) -> Result<CartridgePng, String> {
    if bytes.len() < PNG_SIGNATURE.len()
        || bytes.len() > MAX_CARTRIDGE_PNG_BYTES
        || !bytes.starts_with(&PNG_SIGNATURE)
    {
        return Err("invalid cartridge PNG size or signature".to_owned());
    }
    let mut offset = PNG_SIGNATURE.len();
    let mut payload: Option<&[u8]> = None;
    let mut metadata: Option<CartridgePngMetadata> = None;
    let mut ended = false;
    while offset < bytes.len() {
        if ended || bytes.len() - offset < 12 {
            return Err("truncated or trailing PNG chunk".to_owned());
        }
        let length = usize::try_from(u32::from_be_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .map_err(|_| "chunk length")?,
        ))
        .map_err(|_| "chunk length overflow")?;
        let end = offset
            .checked_add(12)
            .and_then(|value| value.checked_add(length))
            .ok_or("chunk overflow")?;
        if length > MAX_CARTRIDGE_PNG_BYTES || end > bytes.len() {
            return Err("invalid PNG chunk length".to_owned());
        }
        let kind: [u8; 4] = bytes[offset + 4..offset + 8]
            .try_into()
            .map_err(|_| "chunk type")?;
        let data = &bytes[offset + 8..offset + 8 + length];
        let expected = u32::from_be_bytes(
            bytes[offset + 8 + length..end]
                .try_into()
                .map_err(|_| "chunk CRC")?,
        );
        if crc32(&kind, data) != expected {
            return Err("PNG chunk CRC mismatch".to_owned());
        }
        match &kind {
            b"moCa"
                if payload.is_none() && !data.is_empty() && data.len() <= MAX_GAME_PACKED_BYTES =>
            {
                payload = Some(data);
            }
            b"moCa" => return Err("invalid or duplicate cartridge payload".to_owned()),
            b"moCm" if metadata.is_none() && data.len() <= 2048 => {
                metadata = Some(
                    serde_json::from_slice(data).map_err(|_| "invalid cartridge PNG metadata")?,
                );
            }
            b"moCm" => return Err("invalid or duplicate cartridge metadata".to_owned()),
            b"IEND" if data.is_empty() => ended = true,
            _ => {}
        }
        offset = end;
    }
    if !ended {
        return Err("cartridge PNG has no IEND".to_owned());
    }
    let cartridge = payload.ok_or("cartridge PNG has no payload")?;
    let metadata = metadata.ok_or("cartridge PNG has no metadata")?;
    validate(cartridge, &metadata)?;
    Ok(CartridgePng {
        cartridge: cartridge.to_vec(),
        metadata,
    })
}

fn validate(cartridge: &[u8], metadata: &CartridgePngMetadata) -> Result<(), String> {
    if !valid_payload_length(cartridge.len()) {
        return Err("game payload exceeds shipping-container capacity".to_owned());
    }
    if metadata.title.is_empty()
        || metadata.title.len() > 64
        || metadata.author.is_empty()
        || metadata.author.len() > 64
        || !(1970..=9999).contains(&metadata.year)
        || !(1..=4).contains(&metadata.players)
        || metadata.controls.len() > 64
    {
        return Err("invalid cartridge PNG metadata".to_owned());
    }
    decode_game(cartridge).map_err(|error| error.to_string())?;
    Ok(())
}

fn valid_payload_length(length: usize) -> bool {
    length > 0 && length <= MAX_GAME_PACKED_BYTES
}

fn fill(rgba: &mut [u8], left: usize, top: usize, width: usize, height: usize, color: usize) {
    for y in top..top + height {
        for x in left..left + width {
            let offset = (y * WIDTH + x) * 4;
            rgba[offset..offset + 3].copy_from_slice(&PALETTE[color]);
            rgba[offset + 3] = 255;
        }
    }
}

fn chunk(output: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) -> Result<(), String> {
    output.extend_from_slice(
        &u32::try_from(data.len())
            .map_err(|_| "PNG chunk too large")?
            .to_be_bytes(),
    );
    output.extend_from_slice(&kind);
    output.extend_from_slice(data);
    output.extend_from_slice(&crc32(&kind, data).to_be_bytes());
    Ok(())
}

fn zlib_stored(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let blocks = bytes.len().div_ceil(u16::MAX.into()).max(1);
    let mut output = Vec::with_capacity(2 + bytes.len() + blocks * 5 + 4);
    output.extend_from_slice(&[0x78, 0x01]);
    for (index, block) in bytes.chunks(usize::from(u16::MAX)).enumerate() {
        output.push(u8::from(index + 1 == blocks));
        let length = u16::try_from(block.len()).map_err(|_| "DEFLATE block too large")?;
        output.extend_from_slice(&length.to_le_bytes());
        output.extend_from_slice(&(!length).to_le_bytes());
        output.extend_from_slice(block);
    }
    output.extend_from_slice(&adler32(bytes).to_be_bytes());
    Ok(output)
}

fn crc32(kind: &[u8], bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in kind.iter().chain(bytes) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
        }
    }
    crc ^ u32::MAX
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut first, mut second) = (1_u32, 0_u32);
    for byte in bytes {
        first = (first + u32::from(*byte)) % 65_521;
        second = (second + first) % 65_521;
    }
    (second << 16) | first
}

#[cfg(test)]
mod tests {
    use super::{CartridgePngMetadata, decode_cartridge_png, encode_cartridge_png};

    #[test]
    fn cartridge_png_round_trips_and_checks_corruption() {
        let metadata = CartridgePngMetadata {
            title: "TEST".to_owned(),
            author: "@gongahkia".to_owned(),
            year: 1999,
            players: 2,
            controls: "PAD".to_owned(),
        };
        let game =
            crate::game::pack_game_workcart(include_str!("../../../templates/blank/cart.m01w"))
                .unwrap();
        let png = encode_cartridge_png(game.bytes(), &metadata).expect("encode");
        let decoded = decode_cartridge_png(&png).expect("decode");
        assert_eq!(decoded.cartridge, game.bytes());
        assert_eq!(decoded.metadata, metadata);
        let mut corrupt = png;
        corrupt[20] ^= 1;
        assert!(decode_cartridge_png(&corrupt).unwrap_err().contains("CRC"));
    }

    #[test]
    fn png_carries_an_entire_large_game_but_each_console_rom_keeps_its_own_capacity() {
        let mut main =
            crate::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
        main.files.insert(
            "authoring/notes.txt".to_owned(),
            (0..600_000)
                .map(|index| b'a' + u8::try_from((index * 17 + index / 29) % 26).unwrap())
                .collect(),
        );
        let mut child = main.clone();
        child.manifest.id = "png-child".to_owned();
        child.manifest.title = "CHILD TITLE".to_owned();
        let child = crate::encode_workcart(&child.manifest, &child.files, &[], &[]).unwrap();
        let source = crate::workcart::encode_workcart_with_bundle(
            &main.manifest,
            &main.files,
            &[],
            &[],
            &[crate::bundle::BundleSource {
                alias: "child".to_owned(),
                source: child,
            }],
        )
        .unwrap();
        let game = crate::game::pack_game_workcart(&source).unwrap();
        assert!(game.bytes().len() > crate::cartridge::CARTRIDGE_CAPACITY_BYTES);
        assert!(
            game.catalog()
                .cartridges()
                .values()
                .all(|member| member.bytes.len() <= crate::cartridge::CARTRIDGE_CAPACITY_BYTES)
        );
        let png = super::encode_game_png(&game).unwrap();
        let decoded = decode_cartridge_png(&png).unwrap();
        assert_eq!(decoded.cartridge, game.bytes());
        assert_eq!(decoded.metadata.title, "BLANK CART");
        assert_eq!(
            crate::game::decode_game(&decoded.cartridge)
                .unwrap()
                .source(),
            source
        );
        assert_eq!(super::encode_game_png(&game).unwrap(), png);
    }

    #[test]
    fn crc_valid_wrappers_do_not_bypass_current_game_admission_and_bounds_need_no_large_allocation()
    {
        let metadata = CartridgePngMetadata {
            title: "TEST".to_owned(),
            author: "@test".to_owned(),
            year: 1999,
            players: 1,
            controls: "PAD".to_owned(),
        };
        for invalid in [b"M01Ctest".as_slice(), b"MOD-01\x1a\x01", b"MOD-01\x1a\x02"] {
            assert!(encode_cartridge_png(invalid, &metadata).is_err());
        }
        assert_eq!(super::crc32(b"1234", b"56789"), 0xcbf4_3926);
        assert!(!super::valid_payload_length(0));
        assert!(super::valid_payload_length(super::MAX_GAME_PACKED_BYTES));
        assert!(!super::valid_payload_length(
            super::MAX_GAME_PACKED_BYTES + 1
        ));
        let game =
            crate::game::pack_game_workcart(include_str!("../../../templates/blank/cart.m01w"))
                .unwrap();
        let mut png = super::encode_game_png(&game).unwrap();
        let kind = png.windows(4).position(|kind| kind == b"moCa").unwrap();
        let length = u32::from_be_bytes(png[kind - 4..kind].try_into().unwrap()) as usize;
        png[kind + 4] ^= 1;
        let crc = super::crc32(b"moCa", &png[kind + 4..kind + 4 + length]);
        png[kind + 4 + length..kind + 8 + length].copy_from_slice(&crc.to_be_bytes());
        let error = decode_cartridge_png(&png).unwrap_err();
        assert!(!error.contains("CRC"), "{error}");
        assert!(error.contains("M014"), "{error}");
    }
}

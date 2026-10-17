//! Browser-compatible `.m01save` export and import.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

const MAX_SAVE_BYTES: usize = 8192;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SaveExport {
    revision: u8,
    cartridge_id: String,
    schema_version: u16,
    byte_length: usize,
    checksum: String,
    bytes: String,
}

pub fn export(id: &str, schema: u16, bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() > MAX_SAVE_BYTES {
        return Err("save exceeds 8 KiB".to_owned());
    }
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut hex, "{byte:02x}").map_err(|error| error.to_string())?;
    }
    serde_json::to_vec(&SaveExport {
        revision: 1,
        cartridge_id: id.to_owned(),
        schema_version: schema,
        byte_length: bytes.len(),
        checksum: checksum(bytes),
        bytes: hex,
    })
    .map_err(|error| error.to_string())
}

pub fn import(id: &str, source: &[u8]) -> Result<(u16, Vec<u8>), String> {
    if source.is_empty() || source.len() > MAX_SAVE_BYTES * 2 + 1024 {
        return Err("save container size is invalid".to_owned());
    }
    let container: SaveExport =
        serde_json::from_slice(source).map_err(|error| error.to_string())?;
    if container.revision != 1
        || container.cartridge_id != id
        || container.byte_length > MAX_SAVE_BYTES
    {
        return Err("save container identity or schema is invalid".to_owned());
    }
    if container.bytes.len() != container.byte_length * 2
        || !container
            .bytes
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("save bytes are not canonical lowercase hex".to_owned());
    }
    let bytes = (0..container.bytes.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&container.bytes[index..index + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if checksum(&bytes) != container.checksum {
        return Err("save container checksum failed".to_owned());
    }
    Ok((container.schema_version, bytes))
}

fn checksum(bytes: &[u8]) -> String {
    let mut value = !0_u32;
    for &byte in bytes {
        value ^= u32::from(byte);
        for _ in 0..8 {
            value = (value >> 1) ^ (if value & 1 == 1 { 0xedb8_8320 } else { 0 });
        }
    }
    format!("{:08x}", !value)
}

#[cfg(test)]
mod tests {
    use super::{checksum, export, import};

    #[test]
    fn browser_save_container_round_trips_and_checks_identity_and_checksum() {
        assert_eq!(checksum(b"123456789"), "cbf43926");
        let encoded = export("example-cart", 7, &[0, 16, 255]).unwrap();
        assert_eq!(
            import("example-cart", &encoded).unwrap(),
            (7, vec![0, 16, 255])
        );
        assert!(import("different-cart", &encoded).is_err());
        let changed = String::from_utf8(encoded)
            .unwrap()
            .replace("0010ff", "0010fe");
        assert!(import("example-cart", changed.as_bytes()).is_err());
    }
}

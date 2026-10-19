//! One source-visible game: a current cartridge, or a flat composition of current cartridges.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    CARTRIDGE_FORMAT_REVISION, CartridgeError, PackedCartridge,
    bundle::{
        BUNDLE_ENTRY, BundleSource, MAX_BUNDLE_CARTRIDGES, PackedBundle, admitted_catalog,
        pack_bundle_workcarts, valid_alias,
    },
    cartridge::{
        ArchiveLimits, CARTRIDGE_CAPACITY_BYTES, FileIntegrity, decode_archive, decode_cartridge,
        encode_archive_with_limits, first_archive_path, sha256_hex,
    },
    encode_workcart, load_cartridge_program, parse_workcart, unpack_workcart,
    workcart::MAX_WORKCART_DOCUMENT_BYTES,
};

const MAX_INDEX_BYTES: usize = 64 * 1024;
const SOURCE_PATH: &str = "source/workcart.m01w";
const INDEX_PATH: &str = "bundle.json";
const MAX_GAME_UNPACKED_BYTES: usize = MAX_WORKCART_DOCUMENT_BYTES
    + MAX_BUNDLE_CARTRIDGES * CARTRIDGE_CAPACITY_BYTES
    + MAX_INDEX_BYTES;
/// Shipping-container bound. Each logical console ROM still admits at most 1 MiB.
pub const MAX_GAME_PACKED_BYTES: usize =
    MAX_GAME_UNPACKED_BYTES + MAX_GAME_UNPACKED_BYTES.div_ceil(128) + MAX_INDEX_BYTES;
const GAME_LIMITS: ArchiveLimits = ArchiveLimits {
    packed: MAX_GAME_PACKED_BYTES,
    unpacked: MAX_GAME_UNPACKED_BYTES,
    entries: MAX_BUNDLE_CARTRIDGES + 2,
};

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BundleIndex {
    format_revision: u16,
    kind: String,
    entry: String,
    catalog_sha256: String,
    source: FileIntegrity,
    members: BTreeMap<String, FileIntegrity>,
}

/// Private construction guarantees that all members, source and immutable game identity agree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackedGame {
    bytes: Vec<u8>,
    source: String,
    catalog: PackedBundle,
    identity: String,
}

impl PackedGame {
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    #[must_use]
    pub fn catalog(&self) -> &PackedBundle {
        &self.catalog
    }
    /// Identity covers the whole shipped game, including exact enclosing source/comments.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }
    /// Main owns the portable distribution filename, not the selected editor member.
    ///
    /// # Panics
    /// Only if a private constructor violates the admitted cartridge-ID invariant.
    #[must_use]
    pub fn file_stem(&self) -> String {
        crate::cartridge::cartridge_file_stem(&self.entry().manifest.id)
            .expect("admitted game has a valid main cartridge id")
    }
    #[must_use]
    pub fn is_bundle(&self) -> bool {
        self.catalog.cartridges().len() > 1
    }
    /// Returns the initial logical cartridge. The immutable catalog always contains main.
    ///
    /// # Panics
    /// Panics only if an internal constructor violates the private main-entry invariant.
    #[must_use]
    pub fn entry(&self) -> &PackedCartridge {
        self.catalog
            .cartridges()
            .get(BUNDLE_ENTRY)
            .expect("admitted game has main")
    }
}

/// Packs one current workcart, including optional flat embedded dependencies.
/// Ordinary cartridges retain their exact existing `.m01c` bytes. A multi-cart game
/// uses the same canonical M01C/2 archive encoding to compose independently bounded members.
/// No dependency names are interpreted as host paths.
///
/// # Errors
/// Returns ordinary member-qualified source/compiler/asset/capacity errors, or M014070–74
/// for invalid aliases, save identities, nesting, resource limits or bundle encoding.
pub fn pack_game_workcart(source: &str) -> Result<PackedGame, CartridgeError> {
    let project = parse_workcart(source)?;
    let main_source = if project.bundle.is_empty() {
        source.to_owned()
    } else {
        encode_workcart(
            &project.manifest,
            &project.files,
            &project.recipes,
            &project.tests,
        )?
    };
    let mut members = project.bundle;
    members.push(BundleSource {
        alias: BUNDLE_ENTRY.to_owned(),
        source: main_source,
    });
    let catalog = pack_bundle_workcarts(&members)?;
    let bytes = if catalog.cartridges().len() == 1 {
        catalog.cartridges()[BUNDLE_ENTRY].bytes.clone()
    } else {
        encode_bundle(source, &catalog)?
    };
    Ok(PackedGame {
        identity: sha256_hex(&bytes),
        bytes,
        source: source.to_owned(),
        catalog,
    })
}

/// Admits untrusted current game bytes without executing any cartridge JavaScript.
/// Every bundled member is recompiled from the enclosing exact workcart, including unused
/// members. The package must match that canonical reconstruction byte-for-byte.
///
/// # Errors
/// Rejects unknown/historical headers, corrupt/noncanonical archives, unlisted files,
/// forged source/build/member bindings, nested games and all ordinary admission errors.
pub fn decode_game(bytes: &[u8]) -> Result<PackedGame, CartridgeError> {
    if first_archive_path(bytes)? != INDEX_PATH {
        let (decoded, _) = load_cartridge_program(bytes)?;
        let source = unpack_workcart(bytes)?;
        let catalog = admitted_catalog(BTreeMap::from([(
            BUNDLE_ENTRY.to_owned(),
            PackedCartridge {
                bytes: bytes.to_vec(),
                manifest: decoded.manifest,
            },
        )]));
        return Ok(PackedGame {
            identity: sha256_hex(bytes),
            bytes: bytes.to_vec(),
            source,
            catalog,
        });
    }
    let entries = decode_archive(bytes, GAME_LIMITS)?;
    let index_bytes = entries
        .get(INDEX_PATH)
        .ok_or_else(|| error("missing bundle index"))?;
    if index_bytes.len() > MAX_INDEX_BYTES {
        return Err(error("bundle index exceeds 64 KiB"));
    }
    let index: BundleIndex = serde_json::from_slice(index_bytes)
        .map_err(|failure| error(format!("invalid bundle index: {failure}")))?;
    if serde_json::to_vec(&index).ok().as_deref() != Some(index_bytes)
        || index.format_revision != CARTRIDGE_FORMAT_REVISION
        || index.kind != "bundle"
        || index.entry != BUNDLE_ENTRY
        || !(2..=MAX_BUNDLE_CARTRIDGES).contains(&index.members.len())
        || !index.members.contains_key(BUNDLE_ENTRY)
        || entries.len() != index.members.len() + 2
    {
        return Err(error(
            "bundle index is not canonical current flat composition",
        ));
    }
    let source = entries
        .get(SOURCE_PATH)
        .ok_or_else(|| error("missing exact bundle workcart"))?;
    if source.len() > MAX_WORKCART_DOCUMENT_BYTES {
        return Err(error("bundle workcart exceeds 128 MiB"));
    }
    check_file(source, &index.source)?;
    let mut identities = BTreeSet::new();
    for (alias, integrity) in &index.members {
        if !valid_alias(alias) {
            return Err(error(format!("invalid bundle alias '{alias}'")));
        }
        let member = entries
            .get(&member_path(alias))
            .ok_or_else(|| error(format!("missing bundle member '{alias}'")))?;
        check_file(member, integrity)?;
        // The ordinary decoder enforces the 1 MiB logical-cart and current header/schema bounds.
        // A nested composition has no logical manifest and is rejected before any recursion.
        let decoded = decode_cartridge(member).map_err(|mut failure| {
            failure.message = format!("bundle cartridge '{alias}': {}", failure.message);
            failure
        })?;
        if !identities.insert(decoded.manifest.id.clone()) {
            return Err(CartridgeError {
                code: "M014071",
                message: format!(
                    "bundle cartridge '{alias}' reuses save identity '{}'",
                    decoded.manifest.id
                ),
            });
        }
    }
    let source = std::str::from_utf8(source).map_err(|_| error("bundle workcart is not UTF-8"))?;
    let expected = pack_game_workcart(source)?;
    if !expected.is_bundle()
        || expected.catalog.identity() != index.catalog_sha256
        || expected.bytes != bytes
    {
        return Err(error(
            "bundle members/index differ from authoritative source recompilation",
        ));
    }
    Ok(expected)
}

fn encode_bundle(source: &str, catalog: &PackedBundle) -> Result<Vec<u8>, CartridgeError> {
    let mut entries = BTreeMap::from([(SOURCE_PATH.to_owned(), source.as_bytes().to_vec())]);
    let mut members = BTreeMap::new();
    for (alias, cart) in catalog.cartridges() {
        members.insert(alias.clone(), integrity(&cart.bytes)?);
        entries.insert(member_path(alias), cart.bytes.clone());
    }
    let index = BundleIndex {
        format_revision: CARTRIDGE_FORMAT_REVISION,
        kind: "bundle".to_owned(),
        entry: BUNDLE_ENTRY.to_owned(),
        catalog_sha256: catalog.identity().to_owned(),
        source: integrity(source.as_bytes())?,
        members,
    };
    let encoded_index = serde_json::to_vec(&index).map_err(|failure| error(failure.to_string()))?;
    if encoded_index.len() > MAX_INDEX_BYTES {
        return Err(error("bundle index exceeds 64 KiB"));
    }
    entries.insert(INDEX_PATH.to_owned(), encoded_index);
    encode_archive_with_limits(&entries, GAME_LIMITS)
}

fn integrity(bytes: &[u8]) -> Result<FileIntegrity, CartridgeError> {
    Ok(FileIntegrity {
        bytes: u32::try_from(bytes.len()).map_err(|_| error("bundle entry size overflow"))?,
        sha256: sha256_hex(bytes),
    })
}
fn check_file(bytes: &[u8], expected: &FileIntegrity) -> Result<(), CartridgeError> {
    if usize::try_from(expected.bytes).ok() != Some(bytes.len())
        || expected.sha256 != sha256_hex(bytes)
    {
        return Err(error(
            "bundle index entry size/hash does not match its payload",
        ));
    }
    Ok(())
}
fn member_path(alias: &str) -> String {
    format!("cartridges/{alias}.m01c")
}
fn error(message: impl Into<String>) -> CartridgeError {
    CartridgeError {
        code: "M014074",
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "game_tests.rs"]
mod tests;

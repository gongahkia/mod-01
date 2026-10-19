//! Shared admission for local multi-cartridge games. Members retain the current M01W/M01C contract.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    CartridgeError, PackedCartridge, cartridge::sha256_hex, pack_workcart, parse_workcart,
};

/// Stable entry alias, independent of a member's persistent cartridge ID.
pub const BUNDLE_ENTRY: &str = "main";
/// Includes the entry cartridge, not 32 additional cartridges.
pub const MAX_BUNDLE_CARTRIDGES: usize = 32;
/// Authoring allocation bound; packed members still enforce their own 1 MiB limit.
pub const MAX_BUNDLE_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_MEMBER_SOURCE_BYTES: usize = 16 * 1024 * 1024;

/// One authored current-format member, named by a local alias rather than a filesystem path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleSource {
    pub alias: String,
    pub source: String,
}

/// Fully admitted members, deterministically ordered, with an identity covering aliases and bytes.
/// Construction is private so consumers cannot accidentally bypass admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackedBundle {
    cartridges: BTreeMap<String, PackedCartridge>,
    identity: String,
}

impl PackedBundle {
    #[must_use]
    pub fn cartridges(&self) -> &BTreeMap<String, PackedCartridge> {
        &self.cartridges
    }

    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

/// Admits an entire local game before a host writes a package or replaces a running game.
/// This function never reads paths, invokes Node, writes files, or mutates save data.
///
/// # Errors
/// Returns M014070 for invalid counts/aliases, M014071 for duplicate save identities,
/// M014072 for source limits, or a member-qualified ordinary workcart/compiler/asset/
/// packed-capacity error. Input order never changes canonical members or identity.
pub fn pack_bundle_workcarts(sources: &[BundleSource]) -> Result<PackedBundle, CartridgeError> {
    if sources.is_empty() || sources.len() > MAX_BUNDLE_CARTRIDGES {
        return Err(error(
            "M014070",
            "bundle requires 1–32 cartridges including main",
        ));
    }
    let mut ordered = BTreeMap::new();
    let mut total = 0;
    for member in sources {
        if !valid_alias(&member.alias)
            || ordered
                .insert(member.alias.as_str(), member.source.as_str())
                .is_some()
        {
            return Err(error(
                "M014070",
                format!(
                    "invalid or duplicate bundle alias '{}'; use 1–32 lowercase ASCII letters/digits/hyphens/underscores, starting with a letter",
                    member.alias
                ),
            ));
        }
        total = checked_source_size(total, member.source.len())
            .map_err(|error| member_error(&member.alias, error))?;
    }
    if !ordered.contains_key(BUNDLE_ENTRY) {
        return Err(error(
            "M014070",
            "bundle is missing its main entry cartridge",
        ));
    }
    let mut identities = BTreeSet::new();
    // Check every member's metadata before compiling any member, including otherwise unused carts.
    for (alias, source) in &ordered {
        let project = parse_workcart(source).map_err(|error| member_error(alias, error))?;
        if !project.bundle.is_empty() {
            return Err(error(
                "M014073",
                format!("bundle cartridge '{alias}' cannot contain a nested bundle"),
            ));
        }
        if !identities.insert(project.manifest.id.clone()) {
            return Err(error(
                "M014071",
                format!(
                    "bundle cartridge '{alias}' reuses save identity '{}'; every member needs its own cartridge id",
                    project.manifest.id
                ),
            ));
        }
    }
    let mut cartridges = BTreeMap::new();
    for (alias, source) in ordered {
        let packed = pack_workcart(source).map_err(|error| member_error(alias, error))?;
        cartridges.insert(alias.to_owned(), packed);
    }
    Ok(admitted_catalog(cartridges))
}

/// Only call after current member admission and alias/save-identity validation.
pub(crate) fn admitted_catalog(cartridges: BTreeMap<String, PackedCartridge>) -> PackedBundle {
    let mut identity = String::from("MOD-01 local cartridge catalog/1\n");
    for (alias, packed) in &cartridges {
        // Admitted aliases cannot contain ':' or newlines. Hashes commit exact member bytes.
        identity.push_str(alias);
        identity.push(':');
        identity.push_str(&sha256_hex(&packed.bytes));
        identity.push('\n');
    }
    PackedBundle {
        cartridges,
        identity: sha256_hex(identity.as_bytes()),
    }
}

pub(crate) fn valid_alias(alias: &str) -> bool {
    (1..=32).contains(&alias.len())
        && alias.as_bytes()[0].is_ascii_lowercase()
        && alias.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

pub(crate) fn checked_source_size(total: usize, bytes: usize) -> Result<usize, CartridgeError> {
    if bytes > MAX_MEMBER_SOURCE_BYTES {
        return Err(error("M014072", "bundle member source exceeds 16 MiB"));
    }
    total
        .checked_add(bytes)
        .filter(|total| *total <= MAX_BUNDLE_SOURCE_BYTES)
        .ok_or_else(|| error("M014072", "bundle authoring sources exceed 64 MiB"))
}

fn member_error(alias: &str, mut error: CartridgeError) -> CartridgeError {
    error.message = format!("bundle cartridge '{alias}': {}", error.message);
    error
}

fn error(code: &'static str, message: impl Into<String>) -> CartridgeError {
    CartridgeError {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_cartridge_program, unpack_workcart};

    fn member(alias: &str) -> BundleSource {
        let mut project =
            parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
        project.manifest.id = format!("bundle-{alias}");
        BundleSource {
            alias: alias.to_owned(),
            source: crate::encode_workcart(
                &project.manifest,
                &project.files,
                &project.recipes,
                &project.tests,
            )
            .unwrap(),
        }
    }

    #[test]
    fn packs_32_current_cartridges_and_retains_exact_source_independent_of_input_order() {
        let mut sources = vec![member(BUNDLE_ENTRY)];
        sources.extend((1..32).map(|index| member(&format!("level-{index}"))));
        let first = pack_bundle_workcarts(&sources).unwrap();
        assert_eq!(first.cartridges().len(), 32);
        assert_eq!(first.identity().len(), 64);
        for source in &sources {
            let packed = &first.cartridges()[&source.alias];
            assert!(packed.bytes.len() <= crate::cartridge::CARTRIDGE_CAPACITY_BYTES);
            load_cartridge_program(&packed.bytes).unwrap();
            assert_eq!(unpack_workcart(&packed.bytes).unwrap(), source.source);
            assert_eq!(packed.manifest.hardware_revision, 2);
        }
        sources.reverse();
        assert_eq!(pack_bundle_workcarts(&sources).unwrap(), first);
        sources.push(member("too-many"));
        assert_eq!(pack_bundle_workcarts(&sources).unwrap_err().code, "M014070");
    }

    #[test]
    fn catalog_identity_covers_aliases_and_exact_source_not_only_generated_programs() {
        let sources = [member("main"), member("level")];
        let first = pack_bundle_workcarts(&sources).unwrap();
        let mut renamed = sources.clone();
        renamed[1].alias = "another-name".to_owned();
        assert_ne!(
            first.identity(),
            pack_bundle_workcarts(&renamed).unwrap().identity()
        );
        let mut edited = sources.clone();
        edited[1].source.push_str("\n# exact authored source\n");
        assert_ne!(
            first.identity(),
            pack_bundle_workcarts(&edited).unwrap().identity()
        );
    }

    #[test]
    fn rejects_ambiguous_aliases_missing_entry_and_duplicate_persistent_ids() {
        assert!(pack_bundle_workcarts(&[]).is_err());
        assert!(
            pack_bundle_workcarts(&[member("level")])
                .unwrap_err()
                .message
                .contains("main")
        );
        assert!(pack_bundle_workcarts(&[member("main"), member("main")]).is_err());
        for alias in [
            "",
            "Main",
            "1level",
            "../level",
            "a/b",
            "https://cart",
            "a:b",
            "a\nb",
            "å",
            &"a".repeat(33),
        ] {
            let mut invalid = member("level");
            invalid.alias = alias.to_owned();
            assert_eq!(
                pack_bundle_workcarts(&[member("main"), invalid])
                    .unwrap_err()
                    .code,
                "M014070"
            );
        }
        let mut duplicate = member("main");
        duplicate.alias = "different-alias".to_owned();
        let failure = pack_bundle_workcarts(&[member("main"), duplicate]).unwrap_err();
        assert_eq!(failure.code, "M014071");
        assert!(failure.message.contains("save identity"));
    }

    #[test]
    fn rejects_even_unused_member_compiler_asset_and_revision_errors_with_alias_context() {
        for (old, new, code) in [
            ("format = 2", "format = 1", "M014021"),
            ("clear(1)", "missing_builtin(1)", "M014003"),
        ] {
            let mut broken = member("unused");
            // Use the materialized entry for the semantic fixture, not guessed template text.
            if code == "M014003" {
                let mut project = parse_workcart(&broken.source).unwrap();
                project.files.insert(
                    project.manifest.entry.clone(),
                    b"on draw:\n  missing_builtin(1)\n".to_vec(),
                );
                broken.source = crate::encode_workcart(
                    &project.manifest,
                    &project.files,
                    &project.recipes,
                    &project.tests,
                )
                .unwrap();
            } else {
                broken.source = broken.source.replacen(old, new, 1);
            }
            let failure = pack_bundle_workcarts(&[member("main"), broken]).unwrap_err();
            assert_eq!(failure.code, code);
            assert!(failure.message.contains("unused"));
        }
        let mut broken = member("unused-assets");
        let mut project = parse_workcart(&broken.source).unwrap();
        project.manifest.assets.insert(
            "bad".to_owned(),
            crate::ProjectAsset {
                kind: crate::AssetKind::Map,
                path: "assets/bad.m01m".to_owned(),
            },
        );
        project.files.insert(
            "assets/bad.m01m".to_owned(),
            br#"{"revision":1,"tileSet":"missing","width":1,"height":1,"layers":[[0]]}"#.to_vec(),
        );
        broken.source = crate::encode_workcart(
            &project.manifest,
            &project.files,
            &project.recipes,
            &project.tests,
        )
        .unwrap();
        let failure = pack_bundle_workcarts(&[member("main"), broken]).unwrap_err();
        assert!(failure.message.contains("unused-assets"));
    }

    #[test]
    fn preflights_source_allocation_limits_before_parsing_and_checks_overflow() {
        assert_eq!(
            checked_source_size(0, MAX_MEMBER_SOURCE_BYTES).unwrap(),
            MAX_MEMBER_SOURCE_BYTES
        );
        assert_eq!(
            checked_source_size(MAX_BUNDLE_SOURCE_BYTES - 1, 1).unwrap(),
            MAX_BUNDLE_SOURCE_BYTES
        );
        assert!(checked_source_size(0, MAX_MEMBER_SOURCE_BYTES + 1).is_err());
        assert!(checked_source_size(MAX_BUNDLE_SOURCE_BYTES, 1).is_err());
        assert!(checked_source_size(usize::MAX, 1).is_err());
        let oversized = BundleSource {
            alias: "main".to_owned(),
            source: " ".repeat(MAX_MEMBER_SOURCE_BYTES + 1),
        };
        assert_eq!(
            pack_bundle_workcarts(&[oversized]).unwrap_err().code,
            "M014072"
        );
    }
}

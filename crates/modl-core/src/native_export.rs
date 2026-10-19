//! Native desktop distribution. No browser, network server or external archiver is involved.
use crate::{CartridgeError, game::decode_game};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativePlatform {
    Linux,
    Windows,
    Macos,
}

impl NativePlatform {
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }
}

#[derive(Clone, Debug)]
pub struct NativeToolchain {
    pub platform: NativePlatform,
    pub player: PathBuf,
    pub node: PathBuf,
    pub host: PathBuf,
    pub node_license: PathBuf,
}

impl NativeToolchain {
    /// Resolves explicit paths, installed sibling tools, or the current development build.
    /// # Errors
    /// Fails if Node or native toolchain paths cannot be found.
    pub fn discover(
        player: Option<&Path>,
        node: Option<&Path>,
        host: Option<&Path>,
        node_license: Option<&Path>,
    ) -> Result<Self, CartridgeError> {
        let executable = std::env::current_exe().map_err(export_error)?;
        let directory = executable
            .parent()
            .ok_or_else(|| export_error("executable has no directory"))?;
        let player = player.map_or_else(
            || {
                directory.join(if cfg!(windows) {
                    "mod01-studio.exe"
                } else {
                    "mod01-studio"
                })
            },
            Path::to_path_buf,
        );
        let node = if let Some(node) = node {
            node.to_path_buf()
        } else {
            let bundled = directory.join(if cfg!(windows) { "node.exe" } else { "node" });
            if bundled.is_file() {
                bundled
            } else {
                let node_command = std::env::var_os("MOD01_NODE").unwrap_or_else(|| "node".into());
                let result = Command::new(node_command)
                    .args(["-p", "process.execPath"])
                    .output()
                    .map_err(export_error)?;
                if !result.status.success() {
                    return Err(export_error("cannot resolve Node executable"));
                }
                PathBuf::from(
                    String::from_utf8(result.stdout)
                        .map_err(export_error)?
                        .trim(),
                )
            }
        };
        let host = host.map_or_else(
            || {
                let bundled = directory.join("native-host.mjs");
                if bundled.is_file() {
                    bundled
                } else {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../packages/runtime/standalone/native-host.mjs")
                }
            },
            Path::to_path_buf,
        );
        let canonical_node = node.canonicalize().map_err(export_error)?;
        let directory = canonical_node
            .parent()
            .ok_or_else(|| export_error("Node has no parent directory"))?;
        let node_license = node_license.map(Path::to_path_buf).or_else(|| {
            [directory.join("LICENSE"), directory.join("../LICENSE"), directory.join("licenses/Node.txt")]
                .into_iter().find(|path| path.is_file())
        }).ok_or_else(|| export_error("Node license is missing; supply --node-license with that Node distribution's LICENSE"))?;
        Ok(Self {
            platform: NativePlatform::current(),
            player,
            node,
            host,
            node_license,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageManifest<'a> {
    format: u8,
    platform: NativePlatform,
    architecture: &'a str,
    cartridge_id: &'a str,
    cartridge_bytes: usize,
    game_identity: &'a str,
    members: BTreeMap<&'a str, PackageMember<'a>>,
    executable: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageMember<'a> {
    cartridge_id: &'a str,
    rom_bytes: usize,
}

/// Creates a new portable directory (a `.app` inside it on macOS).
/// The exact canonical cartridge, runtime and target binaries travel together.
/// # Errors
/// Refuses existing destinations, malformed games (including unused members), missing tools, mismatched
/// binary platforms/architectures, and I/O errors. A failed I/O copy can leave a partial new directory.
#[allow(clippy::too_many_lines)] // one preflight/copy workflow with an explicitly new destination
pub fn export_native_game(
    output: &Path,
    cartridge: &[u8],
    tools: &NativeToolchain,
) -> Result<PathBuf, CartridgeError> {
    // Admission recompiles every member before creating any output. A shipping
    // game can exceed one ROM without relaxing any logical cartridge's capacity.
    let game = decode_game(cartridge)?;
    let cart = game.entry();
    let architecture = binary_architecture(&tools.player, tools.platform)?;
    if architecture != binary_architecture(&tools.node, tools.platform)? {
        return Err(export_error("player and Node architectures differ"));
    }
    let host = fs::read(&tools.host).map_err(export_error)?;
    let node_license = fs::read(&tools.node_license).map_err(export_error)?;
    if node_license.is_empty() || node_license.len() > 2 * 1024 * 1024 {
        return Err(export_error("invalid Node license file"));
    }
    if host.is_empty()
        || host.len() > 2 * 1024 * 1024
        || !host.windows(7).any(|bytes| bytes == b"process")
    {
        return Err(export_error(
            "native host bundle is missing or invalid; build the runtime first",
        ));
    }
    if output.exists() {
        return Err(export_error(
            "native export destination already exists; choose a new directory",
        ));
    }
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(export_error)?;
    }
    fs::create_dir(output).map_err(export_error)?;
    let id = &cart.manifest.id;
    let executable_name = if tools.platform == NativePlatform::Windows {
        format!("{id}.exe")
    } else {
        id.clone()
    };
    let binaries = if tools.platform == NativePlatform::Macos {
        output.join(format!("{id}.app/Contents/MacOS"))
    } else {
        output.to_path_buf()
    };
    fs::create_dir_all(&binaries).map_err(export_error)?;
    let executable = binaries.join(&executable_name);
    fs::copy(&tools.player, &executable).map_err(export_error)?;
    let node = binaries.join(if tools.platform == NativePlatform::Windows {
        "node.exe"
    } else {
        "node"
    });
    fs::copy(&tools.node, &node).map_err(export_error)?;
    fs::write(binaries.join("native-host.mjs"), host).map_err(export_error)?;
    fs::write(binaries.join("game.m01c"), cartridge).map_err(export_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .map_err(export_error)?;
        fs::set_permissions(&node, fs::Permissions::from_mode(0o755)).map_err(export_error)?;
    }
    if tools.platform == NativePlatform::Macos {
        let title = cart
            .manifest
            .title
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        fs::write(output.join(format!("{id}.app/Contents/Info.plist")), format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>CFBundleIdentifier</key><string>dev.mod01.game.{id}</string><key>CFBundleName</key><string>{title}</string><key>CFBundleExecutable</key><string>{executable_name}</string><key>CFBundlePackageType</key><string>APPL</string><key>LSMinimumSystemVersion</key><string>12.0</string></dict></plist>\n"
        )).map_err(export_error)?;
    }
    let relative = executable
        .strip_prefix(output)
        .map_err(export_error)?
        .to_string_lossy();
    let metadata = PackageManifest {
        format: 1,
        platform: tools.platform,
        architecture,
        cartridge_id: id,
        cartridge_bytes: cartridge.len(),
        game_identity: game.identity(),
        members: game
            .catalog()
            .cartridges()
            .iter()
            .map(|(alias, member)| {
                (
                    alias.as_str(),
                    PackageMember {
                        cartridge_id: &member.manifest.id,
                        rom_bytes: member.bytes.len(),
                    },
                )
            })
            .collect(),
        executable: &relative,
    };
    fs::write(
        output.join("mod01-game.json"),
        serde_json::to_vec_pretty(&metadata).map_err(export_error)?,
    )
    .map_err(export_error)?;
    let save_ids = game
        .catalog()
        .cartridges()
        .values()
        .map(|member| member.manifest.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(output.join("README.txt"), format!("{} / MOD-01 native game\n\nRun {} (or open the .app on macOS). Keep all bundled files together.\nArrows + Z/X/A/S + Q/W + Enter. Escape pauses. SOURCE opens the active cartridge in the complete authored game in Studio.\nAll {} cartridge(s) and their authored source travel in game.m01c. Transfers never fetch files or code.\nSaves are local to MOD-01 and separate for each cartridge id: {save_ids}.\nGame SHA-256: {}.\nThe game/runtime never needs a browser or Node installation.\nUnsigned builds may trigger OS trust prompts; distribution signing is the publisher's responsibility.\n", cart.manifest.title, relative, game.catalog().cartridges().len(), game.identity())).map_err(export_error)?;
    let credits = output.join("licenses");
    fs::create_dir(&credits).map_err(export_error)?;
    fs::write(credits.join("Node.txt"), &node_license).map_err(export_error)?;
    // Also keep it beside Node so an installed game can itself export source-visible games.
    fs::write(binaries.join("LICENSE"), &node_license).map_err(export_error)?;
    for (name, contents) in [
        (
            "VISUALS.md",
            include_str!("../../mod01-native/assets/ATTRIBUTION.md"),
        ),
        (
            "BigBlueTerm.txt",
            include_str!("../../mod01-native/assets/bigblue/LICENSE.TXT"),
        ),
        (
            "Kenney.txt",
            include_str!("../../mod01-native/assets/kenney/License.txt"),
        ),
        (
            "Library-renderer.txt",
            include_str!("../../mod01-native/assets/LIBRARY-LICENSES.txt"),
        ),
    ] {
        fs::write(credits.join(name), contents).map_err(export_error)?;
    }
    Ok(executable)
}

fn binary_architecture(
    path: &Path,
    platform: NativePlatform,
) -> Result<&'static str, CartridgeError> {
    let mut header = [0_u8; 4096];
    let count = fs::File::open(path)
        .map_err(export_error)?
        .read(&mut header)
        .map_err(export_error)?;
    if count < 64 {
        return Err(export_error(format!(
            "{} is not a native executable",
            path.display()
        )));
    }
    let architecture = match platform {
        NativePlatform::Linux
            if header.starts_with(b"\x7fELF") && header[4] == 2 && header[5] == 1 =>
        {
            u16::from_le_bytes([header[18], header[19]])
        }
        NativePlatform::Windows if header.starts_with(b"MZ") => {
            let offset = usize::try_from(u32::from_le_bytes(
                header[60..64].try_into().expect("fixed header"),
            ))
            .map_err(export_error)?;
            if offset > count.saturating_sub(6) || &header[offset..offset + 4] != b"PE\0\0" {
                return Err(export_error("invalid PE executable"));
            }
            u16::from_le_bytes([header[offset + 4], header[offset + 5]])
        }
        NativePlatform::Macos if header.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]) => {
            match u32::from_le_bytes(header[4..8].try_into().expect("fixed header")) {
                0x0100_0007 => 62,
                0x0100_000c => 183,
                _ => 0,
            }
        }
        _ => {
            return Err(export_error(format!(
                "{} does not match the selected platform",
                path.display()
            )));
        }
    };
    match architecture {
        62 | 0x8664 => Ok("x86_64"),
        183 | 0xaa64 => Ok("aarch64"),
        _ => Err(export_error("only 64-bit desktop binaries are supported")),
    }
}

fn export_error(error: impl std::fmt::Display) -> CartridgeError {
    CartridgeError {
        code: "M014030",
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_tools(root: &Path, platform: NativePlatform) -> NativeToolchain {
        fs::create_dir_all(root).unwrap();
        let mut header = [0_u8; 128];
        match platform {
            NativePlatform::Linux => {
                header[..6].copy_from_slice(b"\x7fELF\x02\x01");
                header[18] = 62;
            }
            NativePlatform::Windows => {
                header[..2].copy_from_slice(b"MZ");
                header[60] = 80;
                header[80..86].copy_from_slice(b"PE\0\0\x64\x86");
            }
            NativePlatform::Macos => {
                header[..8].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1]);
            }
        }
        let player = root.join("player");
        fs::write(&player, header).unwrap();
        let host = root.join("host.mjs");
        fs::write(&host, "process.stdin; // native host fixture").unwrap();
        let node_license = root.join("Node-LICENSE");
        fs::write(&node_license, "Node license fixture").unwrap();
        NativeToolchain {
            platform,
            player: player.clone(),
            node: player,
            host,
            node_license,
        }
    }

    #[test]
    fn native_packages_keep_large_complete_games_main_identity_and_each_members_own_rom_size() {
        let root =
            std::env::temp_dir().join(format!("mod01-native-whole-game-{}", std::process::id()));
        let mut main =
            crate::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
        main.files.insert(
            "authoring/notes.txt".to_owned(),
            (0..600_000)
                .map(|index| b'a' + u8::try_from((index * 17 + index / 29) % 26).unwrap())
                .collect(),
        );
        let mut child = main.clone();
        child.manifest.id = "native-child".to_owned();
        child.manifest.title = "CHILD TITLE".to_owned();
        let source = crate::workcart::encode_workcart_with_bundle(
            &main.manifest,
            &main.files,
            &[],
            &[],
            &[crate::bundle::BundleSource {
                alias: "child".to_owned(),
                source: crate::encode_workcart(&child.manifest, &child.files, &[], &[]).unwrap(),
            }],
        )
        .unwrap();
        let game = crate::game::pack_game_workcart(&source).unwrap();
        assert!(game.bytes().len() > crate::cartridge::CARTRIDGE_CAPACITY_BYTES);
        for platform in [
            NativePlatform::Linux,
            NativePlatform::Windows,
            NativePlatform::Macos,
        ] {
            let directory = root.join(format!("{platform:?}"));
            let tools = fixture_tools(&directory, platform);
            let output = directory.join("game");
            let executable = export_native_game(&output, game.bytes(), &tools).unwrap();
            let binaries = executable.parent().unwrap();
            assert_eq!(fs::read(binaries.join("game.m01c")).unwrap(), game.bytes());
            assert_eq!(
                fs::read(&executable).unwrap(),
                fs::read(&tools.player).unwrap()
            );
            let metadata: serde_json::Value =
                serde_json::from_slice(&fs::read(output.join("mod01-game.json")).unwrap()).unwrap();
            assert_eq!(metadata["cartridgeId"], "blank-cart");
            assert_eq!(metadata["gameIdentity"], game.identity());
            assert_eq!(metadata["cartridgeBytes"], game.bytes().len());
            assert_eq!(metadata["members"].as_object().unwrap().len(), 2);
            for (alias, member) in game.catalog().cartridges() {
                assert_eq!(
                    metadata["members"][alias]["cartridgeId"],
                    member.manifest.id
                );
                assert_eq!(metadata["members"][alias]["romBytes"], member.bytes.len());
                assert!(member.bytes.len() < crate::cartridge::CARTRIDGE_CAPACITY_BYTES);
            }
            let readme = fs::read_to_string(output.join("README.txt")).unwrap();
            assert!(readme.contains("BLANK CART / MOD-01"));
            assert!(readme.contains("blank-cart"));
            assert!(readme.contains("native-child"));
            assert!(readme.contains(game.identity()));
            if platform == NativePlatform::Macos {
                let plist =
                    fs::read_to_string(output.join("blank-cart.app/Contents/Info.plist")).unwrap();
                assert!(plist.contains("<string>BLANK CART</string>"));
                assert!(executable.starts_with(output.join("blank-cart.app/Contents/MacOS")));
            }
            assert!(export_native_game(&output, game.bytes(), &tools).is_err());
            assert_eq!(fs::read(binaries.join("game.m01c")).unwrap(), game.bytes());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_export_rejects_rehashed_archived_javascript_not_produced_by_its_source_before_creating_output()
     {
        use crate::cartridge::{
            CARTRIDGE_ARCHIVE_LIMITS, FileIntegrity, encode_archive_with_limits, sha256_hex,
        };
        let root = std::env::temp_dir().join(format!("mod01-native-forged-{}", std::process::id()));
        let tools = fixture_tools(&root, NativePlatform::Linux);
        let game =
            crate::game::pack_game_workcart(include_str!("../../../templates/blank/cart.m01w"))
                .unwrap();
        let mut decoded = crate::decode_cartridge(game.bytes()).unwrap();
        let forged =
            b"export default () => { throw new Error('untrusted archived build'); };".to_vec();
        let integrity = FileIntegrity {
            bytes: u32::try_from(forged.len()).unwrap(),
            sha256: sha256_hex(&forged),
        };
        decoded
            .manifest
            .files
            .insert("build/cartridge.js".to_owned(), integrity.clone());
        decoded.manifest.compiled_program = Some(integrity);
        decoded
            .entries
            .insert("build/cartridge.js".to_owned(), forged);
        decoded.entries.insert(
            "manifest.json".to_owned(),
            serde_json::to_vec(&decoded.manifest).unwrap(),
        );
        let bytes = encode_archive_with_limits(&decoded.entries, CARTRIDGE_ARCHIVE_LIMITS).unwrap();
        assert!(
            crate::decode_cartridge(&bytes).is_ok(),
            "ordinary archive integrity is valid"
        );
        let output = root.join("game");
        assert_eq!(
            export_native_game(&output, &bytes, &tools)
                .unwrap_err()
                .code,
            "M014015"
        );
        assert!(!output.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recognizes_target_headers_and_refuses_other_architectures() {
        let path = std::env::temp_dir().join(format!("mod01-native-header-{}", std::process::id()));
        let mut header = [0_u8; 128];
        header[..4].copy_from_slice(b"MZ\0\0");
        header[60] = 80;
        header[80..86].copy_from_slice(b"PE\0\0\x64\x86");
        fs::write(&path, header).unwrap();
        assert_eq!(
            binary_architecture(&path, NativePlatform::Windows).unwrap(),
            "x86_64"
        );
        assert!(binary_architecture(&path, NativePlatform::Linux).is_err());
        header.fill(0);
        header[..8].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1]);
        fs::write(&path, header).unwrap();
        assert_eq!(
            binary_architecture(&path, NativePlatform::Macos).unwrap(),
            "aarch64"
        );
        header[4] = 0;
        fs::write(&path, header).unwrap();
        assert!(binary_architecture(&path, NativePlatform::Macos).is_err());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn native_export_preserves_the_cart_and_refuses_mismatches_and_overwrites() {
        let root = std::env::temp_dir().join(format!("mod01-native-export-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut header = [0_u8; 64];
        header[..6].copy_from_slice(b"\x7fELF\x02\x01");
        header[18] = 62;
        let player = root.join("player");
        fs::write(&player, header).unwrap();
        let host = root.join("host.mjs");
        fs::write(&host, "process.stdin; // native host fixture").unwrap();
        let tools = NativeToolchain {
            platform: NativePlatform::Linux,
            player: player.clone(),
            node: player,
            host,
            node_license: root.join("Node-LICENSE"),
        };
        fs::write(&tools.node_license, "Node license fixture").unwrap();
        let files = BTreeMap::from([(
            "src/main.modl".to_owned(),
            b"on draw:\n  clear(1)\n".to_vec(),
        )]);
        let cart = crate::pack_project("format=2\nlanguage=\"MODL/1\"\nid=\"export-test\"\ntitle=\"Export test\"\nauthor=\"Tester\"\nversion=\"0.1.0\"\nentry=\"src/main.modl\"\nupdate_rate=60\n", &files).unwrap();
        let output = root.join("game");
        assert!(export_native_game(&output, b"invalid", &tools).is_err());
        assert!(!output.exists());
        let mismatched = NativeToolchain {
            platform: NativePlatform::Windows,
            ..tools.clone()
        };
        assert!(export_native_game(&output, &cart.bytes, &mismatched).is_err());
        assert!(!output.exists());
        let executable = export_native_game(&output, &cart.bytes, &tools).unwrap();
        assert!(executable.is_file());
        assert_eq!(fs::read(output.join("game.m01c")).unwrap(), cart.bytes);
        assert!(output.join("node").is_file());
        assert!(output.join("licenses/Kenney.txt").is_file());
        assert!(export_native_game(&output, &cart.bytes, &tools).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

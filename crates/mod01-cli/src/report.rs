//! Analysis of an admitted game. Shipping bytes and individual console capacities
//! are separate; runtime evidence comes from one real whole-game execution.
use std::collections::BTreeMap;

use modl_core::{AssetKind, PackedCartridge, assets, game::PackedGame};
use serde_json::{Value, json};

pub fn game_report(
    game: &PackedGame,
    profile: Option<Result<Value, String>>,
) -> Result<Value, String> {
    let mut members = BTreeMap::new();
    for (alias, member) in game.catalog().cartridges() {
        members.insert(
            alias,
            member_report(member)
                .map_err(|error| format!("bundle cartridge '{alias}': {error}"))?,
        );
    }
    let runtime = runtime_report(game, profile);
    let mut warnings = Vec::new();
    match runtime["status"].as_str() {
        Some("not_requested") => warnings.push("runtime profiling was not requested".to_owned()),
        Some("unavailable") => warnings
            .push("runtime profiling is unavailable; inspect runtime60Frames.error".to_owned()),
        Some("faulted") => warnings.push(format!(
            "runtime profile faulted in '{}'; inspect runtime60Frames.fault",
            runtime["fault"]["cartridgeAlias"]
                .as_str()
                .unwrap_or("unknown")
        )),
        _ => {}
    }
    if runtime["summary"]["workPeak"].as_u64().unwrap_or(0) > 40_000 {
        warnings.push("observed work peak is above 80% of 50,000; inspect the qualified frame in the debugger".to_owned());
    }
    let mut report =
        serde_json::to_value(&game.entry().manifest).map_err(|error| error.to_string())?;
    report
        .as_object_mut()
        .ok_or("entry metadata is not an object")?
        .insert(
            "analysis".to_owned(),
            json!({
                "revision":2,"gameIdentity":game.identity(),"entry":"main",
                "shipping":archive_analysis(game.bytes())?,"members":members,
                "runtime60Frames":runtime,"warnings":warnings,
            }),
        );
    Ok(report)
}

fn member_report(member: &PackedCartridge) -> Result<Value, String> {
    let project =
        modl_core::unpack_cartridge_project(&member.bytes).map_err(|error| error.to_string())?;
    let manifest =
        modl_core::parse_project_manifest(&project.manifest).map_err(|error| error.to_string())?;
    let compilation = modl_core::compile_project(
        &project.manifest,
        &project.files,
        modl_core::CompileMode::Release,
    )
    .map_err(|error| error.to_string())?;
    let generated = compilation
        .generated
        .ok_or("release compilation produced no program")?;
    let asset_report = assets::analyze_project_assets(&manifest, &project.files);
    let usage = asset_report
        .require_valid()
        .map_err(|error| error.to_string())?;
    let mut visual_source = 0;
    let mut audio_source = 0;
    let mut map_source = 0;
    let mut font_source = 0;
    for asset in manifest.assets.values() {
        let length = project.files.get(&asset.path).map_or(0, Vec::len);
        match asset.kind {
            AssetKind::Map => {
                map_source += length;
                visual_source += length;
            }
            AssetKind::Font => {
                font_source += length;
                visual_source += length;
            }
            AssetKind::Sound | AssetKind::Music => audio_source += length,
            _ => visual_source += length,
        }
    }
    let mut symbols = compilation.analysis.ir.as_ref().map(|ir| ir.routines.iter().map(|routine|
        json!({"linkedName":routine.name,"linkedSourceBytes":routine.span.end.saturating_sub(routine.span.start)})
    ).collect::<Vec<_>>()).unwrap_or_default();
    symbols.sort_by(|left, right| {
        right["linkedSourceBytes"]
            .as_u64()
            .cmp(&left["linkedSourceBytes"].as_u64())
            .then_with(|| {
                left["linkedName"]
                    .as_str()
                    .cmp(&right["linkedName"].as_str())
            })
    });
    symbols.truncate(8);
    let mut warnings = Vec::new();
    if member.bytes.len() > 786_432 {
        warnings.push("logical ROM is above 75% of 1 MiB");
    }
    if usage.visual_bytes > 786_432 {
        warnings.push("visual bank is above 75% of 1 MiB");
    }
    if usage.visual_allocations > 6144 {
        warnings.push("visual allocation slots are above 75% of 8192");
    }
    let size_class = match member.bytes.len() {
        0..=4096 => "4K",
        4097..=16_384 => "16K",
        16_385..=65_536 => "64K",
        65_537..=262_144 => "256K",
        _ => "1M",
    };
    Ok(json!({
        "manifest":member.manifest,"romBytes":member.bytes.len(),"sizeClass":size_class,
        "limits":{"romBytes":modl_core::cartridge::CARTRIDGE_CAPACITY_BYTES,
            "visualBytes":assets::VISUAL_CAPACITY_BYTES,"visualAllocations":assets::VISUAL_ALLOCATION_CAPACITY,"saveBytes":8192},
        "assetUsage":usage,
        "sections":{
            "modlSourceBytes":project.files.iter().filter(|(path, _)| std::path::Path::new(path).extension().is_some_and(|extension| extension == "modl")).map(|(_, bytes)| bytes.len()).sum::<usize>(),
            "generatedReleaseBytes":generated.generated_bytes,"visualAssetSourceBytes":visual_source,
            "mapAssetSourceBytes":map_source,"fontAssetSourceBytes":font_source,"audioAssetSourceBytes":audio_source,
        },
        "archive":archive_analysis(&member.bytes)?,"largestSymbols":symbols,"warnings":warnings,
    }))
}

fn runtime_report(game: &PackedGame, profile: Option<Result<Value, String>>) -> Value {
    let Some(result) = profile else {
        return json!({"status":"not_requested"});
    };
    let runtime = match result {
        Err(error) => return json!({"status":"unavailable","error":error}),
        Ok(runtime) => runtime,
    };
    let frames = runtime["frames"].as_array();
    let fault = runtime.get("fault").filter(|fault| !fault.is_null());
    if runtime["game"]["identity"] != game.identity()
        || frames.is_none()
        || runtime["configuration"]["requestedFrames"] != 60
        || runtime["configuration"]["seed"] != modl_core::DEFAULT_SEED
        || runtime["configuration"]["parameter"] != ""
        || runtime["configuration"]["debug"] != false
        || frames.is_some_and(|frames| {
            frames.len() > 60
                || (frames.len() != 60 && fault.is_none())
                || (frames.len() == 60 && fault.is_some())
        })
        || runtime["summary"]["completedFrames"].as_u64()
            != frames.map(|frames| frames.len() as u64)
    {
        return json!({"status":"unavailable","error":"headless profile returned inconsistent game/frame metadata"});
    }
    if let Some(fault) = fault {
        let producer = fault["cartridgeAlias"]
            .as_str()
            .and_then(|alias| game.catalog().cartridges().get(alias));
        if fault["frame"].as_u64() != frames.map(|frames| frames.len() as u64)
            || !producer.is_some_and(|member| fault["cartridgeId"] == member.manifest.id)
            || !fault["code"].is_string()
        {
            return json!({"status":"unavailable","error":"headless profile returned inconsistent fault metadata"});
        }
    }
    let mut member_frames = BTreeMap::<&str, usize>::new();
    for (index, frame) in frames.into_iter().flatten().enumerate() {
        if frame["frame"] != index {
            return json!({"status":"unavailable","error":"headless profile has an inconsistent frame sequence"});
        }
        let Some(alias) = frame["cartridgeAlias"].as_str() else {
            return json!({"status":"unavailable","error":"headless profile omitted a frame producer"});
        };
        if !game
            .catalog()
            .cartridges()
            .get(alias)
            .is_some_and(|member| frame["cartridgeId"] == member.manifest.id)
        {
            return json!({"status":"unavailable","error":"headless profile has an unknown frame producer"});
        }
        *member_frames.entry(alias).or_default() += 1;
    }
    let not_profiled = game
        .catalog()
        .cartridges()
        .keys()
        .filter(|alias| !member_frames.contains_key(alias.as_str()))
        .collect::<Vec<_>>();
    json!({"status":if fault.is_some() {"faulted"} else {"completed"},
        "configuration":runtime["configuration"],"summary":runtime["summary"],"fault":fault,
        "memberFrames":member_frames,"notProfiledMembers":not_profiled})
}

/// Only called with archives already admitted by the shared core.
fn archive_analysis(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() < 12 {
        return Err("archive header is truncated".to_owned());
    }
    let count = u32::from_le_bytes(
        bytes[8..12]
            .try_into()
            .map_err(|_| "invalid archive count")?,
    );
    let mut cursor = 12_usize;
    let mut entries = Vec::new();
    let mut raw_bytes = 0_u64;
    let mut encoded_bytes = 0_u64;
    for _ in 0..count {
        let header = bytes
            .get(cursor..cursor.saturating_add(42))
            .ok_or("archive entry header is truncated")?;
        let path_length = usize::from(u16::from_le_bytes(
            header[..2]
                .try_into()
                .map_err(|_| "invalid archive path length")?,
        ));
        let raw = u32::from_le_bytes(header[2..6].try_into().map_err(|_| "invalid raw length")?);
        let encoded = u32::from_le_bytes(
            header[6..10]
                .try_into()
                .map_err(|_| "invalid encoded length")?,
        );
        cursor += 42;
        let path = std::str::from_utf8(
            bytes
                .get(cursor..cursor.saturating_add(path_length))
                .ok_or("archive path is truncated")?,
        )
        .map_err(|error| error.to_string())?;
        let encoded_size = usize::try_from(encoded).map_err(|error| error.to_string())?;
        cursor = cursor
            .checked_add(path_length)
            .and_then(|cursor| cursor.checked_add(encoded_size))
            .filter(|cursor| *cursor <= bytes.len())
            .ok_or("archive payload is truncated")?;
        raw_bytes += u64::from(raw);
        encoded_bytes += u64::from(encoded);
        entries.push((path, raw));
    }
    entries.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));
    entries.truncate(8);
    Ok(
        json!({"canonicalBytes":bytes.len(),"rawArchiveBytes":raw_bytes,"encodedArchiveBytes":encoded_bytes,
        "containerOverheadBytes":bytes.len() as u64 - encoded_bytes,
        "compressionGainBytes":i64::try_from(raw_bytes).map_err(|error| error.to_string())? - i64::try_from(encoded_bytes).map_err(|error| error.to_string())?,
        "largestEntries":entries.into_iter().map(|(path, bytes)| json!({"path":path,"rawBytes":bytes})).collect::<Vec<_>>() }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiling_never_labels_inconsistent_or_partial_evidence_as_completed() {
        let game =
            modl_core::game::pack_game_workcart(include_str!("../../../templates/blank/cart.m01w"))
                .unwrap();
        let frames = (0..60)
            .map(|frame| json!({"frame":frame,"cartridgeAlias":"main","cartridgeId":"blank-cart"}))
            .collect::<Vec<_>>();
        let valid = json!({"game":{"identity":game.identity()},"configuration":{"requestedFrames":60,"seed":modl_core::DEFAULT_SEED,"parameter":"","debug":false},"frames":frames,"summary":{"completedFrames":60}});
        assert_eq!(
            runtime_report(&game, Some(Ok(valid.clone())))["status"],
            "completed"
        );
        let mut invalid = valid.clone();
        invalid["frames"] = json!([]);
        invalid["summary"]["completedFrames"] = 0.into();
        assert_eq!(
            runtime_report(&game, Some(Ok(invalid.clone())))["status"],
            "unavailable"
        );
        invalid["fault"] =
            json!({"frame":0,"cartridgeAlias":"main","cartridgeId":"blank-cart","code":"M019001"});
        assert_eq!(
            runtime_report(&game, Some(Ok(invalid.clone())))["status"],
            "faulted"
        );
        invalid["fault"]["cartridgeId"] = "foreign-cart".into();
        assert_eq!(
            runtime_report(&game, Some(Ok(invalid)))["status"],
            "unavailable"
        );
        for (key, value) in [
            ("game", json!({"identity":"wrong"})),
            ("configuration", json!({"requestedFrames":1})),
            ("summary", json!({"completedFrames":0})),
        ] {
            let mut invalid = valid.clone();
            invalid[key] = value;
            assert_eq!(
                runtime_report(&game, Some(Ok(invalid)))["status"],
                "unavailable"
            );
        }
        for (key, value) in [
            ("frame", json!(1)),
            ("cartridgeAlias", json!("foreign")),
            ("cartridgeId", json!("foreign-cart")),
        ] {
            let mut invalid = valid.clone();
            invalid["frames"][0][key] = value;
            assert_eq!(
                runtime_report(&game, Some(Ok(invalid)))["status"],
                "unavailable"
            );
        }
        assert_eq!(runtime_report(&game, None)["status"], "not_requested");
        assert_eq!(
            runtime_report(&game, Some(Err("missing Node".to_owned())))["status"],
            "unavailable"
        );
    }
}

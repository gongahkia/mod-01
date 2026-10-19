//! Current asset schemas, cross-asset references and exact visual-image capacity.
//! Authoring tools share this report; the production runtime still validates untrusted loads.
use crate::{AssetKind, CartridgeError, ProjectManifest};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

pub const VISUAL_CAPACITY_BYTES: usize = 1024 * 1024;
pub const VISUAL_ALLOCATION_CAPACITY: usize = 8192;
const MAX_DIAGNOSTICS: usize = 128;
const MAX_ASSET_SOURCE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDiagnostic {
    pub code: &'static str,
    pub asset: String,
    pub path: String,
    pub field: String,
    pub message: String,
}

impl fmt::Display for AssetDiagnostic {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "{}:{}: asset '{}': {}",
            self.path, self.field, self.asset, self.message
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetUsage {
    pub visual_bytes: usize,
    pub visual_allocations: usize,
    pub visual_assets: usize,
    pub audio_assets: usize,
    pub source_bytes: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetReport {
    pub usage: AssetUsage,
    pub diagnostics: Vec<AssetDiagnostic>,
    pub diagnostics_truncated: bool,
}

impl AssetReport {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// # Errors
    /// Returns the first precise diagnostic, retaining its stable error code.
    pub fn require_valid(&self) -> Result<&AssetUsage, CartridgeError> {
        if let Some(diagnostic) = self.diagnostics.first() {
            Err(CartridgeError {
                code: diagnostic.code,
                message: diagnostic.to_string(),
            })
        } else {
            Ok(&self.usage)
        }
    }

    fn push(&mut self, diagnostic: AssetDiagnostic) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(diagnostic);
        } else {
            self.diagnostics_truncated = true;
        }
    }
}

#[derive(Default)]
struct Size {
    bytes: usize,
    allocations: usize,
}

struct Schema<'a> {
    name: &'a str,
    path: &'a str,
}

impl Schema<'_> {
    fn diagnostic(
        &self,
        code: &'static str,
        field: &str,
        message: impl Into<String>,
    ) -> AssetDiagnostic {
        AssetDiagnostic {
            code,
            asset: self.name.to_owned(),
            path: self.path.to_owned(),
            field: field.to_owned(),
            message: message.into(),
        }
    }

    fn expect(&self, valid: bool, field: &str, message: &str) -> Result<(), AssetDiagnostic> {
        if valid {
            Ok(())
        } else {
            Err(self.diagnostic("M014060", field, message))
        }
    }

    // All callers request nonnegative values bounded to at most 65535.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn integer(
        &self,
        value: &Value,
        field: &str,
        minimum: i32,
        maximum: i32,
    ) -> Result<usize, AssetDiagnostic> {
        self.expect(
            integer(value, minimum, maximum),
            field,
            &format!("expected an integer in {minimum}..={maximum}"),
        )?;
        Ok(value.as_f64().unwrap() as usize)
    }

    fn array<'a>(
        &self,
        value: &'a Value,
        field: &str,
        minimum: usize,
        maximum: usize,
    ) -> Result<&'a [Value], AssetDiagnostic> {
        self.expect(
            value
                .as_array()
                .is_some_and(|values| (minimum..=maximum).contains(&values.len())),
            field,
            &format!("expected an array containing {minimum}..={maximum} entries"),
        )?;
        Ok(value.as_array().unwrap())
    }

    fn pixels(
        &self,
        value: &Value,
        field: &str,
        length: usize,
        maximum: i32,
    ) -> Result<(), AssetDiagnostic> {
        let values = self.array(value, field, length, length)?;
        if let Some(index) = values.iter().position(|value| !integer(value, 0, maximum)) {
            return Err(self.diagnostic(
                "M014060",
                &format!("{field}[{index}]"),
                format!("expected an integer in 0..={maximum}"),
            ));
        }
        Ok(())
    }

    fn validate(&self, kind: AssetKind, value: &Value) -> Result<Size, AssetDiagnostic> {
        let (expected_kind, revision) = match kind {
            AssetKind::Sprite | AssetKind::Animation => ("sprite", 2),
            AssetKind::TileSet => ("tile_set", 2),
            AssetKind::Map => ("map", 1),
            AssetKind::Font => ("font", 1),
            AssetKind::Sound => ("sound", 1),
            AssetKind::Music => ("music", 1),
        };
        self.expect(
            value.is_object()
                && value["kind"] == expected_kind
                && integer(&value["revision"], revision, revision),
            "$",
            &format!("expected current {expected_kind} revision {revision}"),
        )?;
        match kind {
            AssetKind::Sprite | AssetKind::Animation => {
                self.sprite(value, kind == AssetKind::Animation)
            }
            AssetKind::TileSet => self.tiles(value),
            AssetKind::Map => self.map(value),
            AssetKind::Font => self.font(value),
            AssetKind::Sound => {
                self.sound(value)?;
                Ok(Size::default())
            }
            AssetKind::Music => {
                self.music(value)?;
                Ok(Size::default())
            }
        }
    }

    fn sprite(&self, value: &Value, animation: bool) -> Result<Size, AssetDiagnostic> {
        let width = self.integer(&value["width"], "$.width", 1, 128)?;
        let height = self.integer(&value["height"], "$.height", 1, 128)?;
        let frames = self.array(&value["frames"], "$.frames", 1, 256)?;
        for (index, frame) in frames.iter().enumerate() {
            self.pixels(frame, &format!("$.frames[{index}]"), width * height, 255)?;
        }
        let count = if animation { frames.len() } else { 1 };
        Ok(Size {
            bytes: width * height * count,
            allocations: count,
        })
    }

    fn tiles(&self, value: &Value) -> Result<Size, AssetDiagnostic> {
        let tiles = self.array(&value["tiles"], "$.tiles", 1, 4096)?;
        self.pixels(&value["flags"], "$.flags", tiles.len(), 255)?;
        for (index, tile) in tiles.iter().enumerate() {
            self.pixels(tile, &format!("$.tiles[{index}]"), 64, 255)?;
        }
        Ok(Size {
            bytes: tiles.len() * 65,
            allocations: tiles.len() + 1,
        })
    }

    fn map(&self, value: &Value) -> Result<Size, AssetDiagnostic> {
        let layers = self.array(&value["layers"], "$.layers", 1, 8)?;
        let mut bytes = 0;
        for (index, layer) in layers.iter().enumerate() {
            let field = format!("$.layers[{index}]");
            let width = self.integer(&layer["width"], &format!("{field}.width"), 1, 256)?;
            let height = self.integer(&layer["height"], &format!("{field}.height"), 1, 256)?;
            self.expect(
                layer["tileSet"].is_string(),
                &format!("{field}.tileSet"),
                "expected a declared tile-set name",
            )?;
            self.pixels(
                &layer["cells"],
                &format!("{field}.cells"),
                width * height,
                65_535,
            )?;
            bytes += width * height * 2;
        }
        Ok(Size {
            bytes,
            allocations: layers.len(),
        })
    }

    fn font(&self, value: &Value) -> Result<Size, AssetDiagnostic> {
        let width = self.integer(&value["glyphWidth"], "$.glyphWidth", 1, 16)?;
        let height = self.integer(&value["glyphHeight"], "$.glyphHeight", 1, 16)?;
        self.integer(
            &value["baseline"],
            "$.baseline",
            0,
            i32::try_from(height).unwrap() - 1,
        )?;
        self.integer(&value["advanceX"], "$.advanceX", 1, 32)?;
        self.integer(&value["advanceY"], "$.advanceY", 1, 32)?;
        let missing = self.integer(&value["missingGlyph"], "$.missingGlyph", 0, 255)?;
        let glyphs = self.array(&value["glyphs"], "$.glyphs", 1, 256)?;
        let mut previous = None;
        let mut fallback = false;
        for (index, glyph) in glyphs.iter().enumerate() {
            let field = format!("$.glyphs[{index}]");
            let code = self.integer(&glyph["code"], &format!("{field}.code"), 0, 255)?;
            self.expect(
                previous.is_none_or(|previous| code > previous),
                &format!("{field}.code"),
                "glyph codes must be unique and strictly increasing",
            )?;
            previous = Some(code);
            fallback |= code == missing;
            self.pixels(
                &glyph["pixels"],
                &format!("{field}.pixels"),
                width * height,
                1,
            )?;
        }
        self.expect(fallback, "$.missingGlyph", "fallback glyph is not present")?;
        Ok(Size {
            bytes: 8 + glyphs.len() * (2 + width * height),
            allocations: 1,
        })
    }

    fn sound(&self, value: &Value) -> Result<(), AssetDiagnostic> {
        self.expect(
            matches!(
                value["waveform"].as_str(),
                Some("pulse" | "triangle" | "saw" | "noise" | "wavetable")
            ),
            "$.waveform",
            "expected pulse, triangle, saw, noise or wavetable",
        )?;
        self.integer(&value["note"], "$.note", 0, 127)?;
        let duration = self.integer(&value["durationFrames"], "$.durationFrames", 1, 3600)?;
        for (field, number, minimum, maximum) in [
            ("$.volume", &value["volume"], 0.0, 1.0),
            ("$.pan", &value["pan"], -1.0, 1.0),
            (
                "$.envelope.sustainLevel",
                &value["envelope"]["sustainLevel"],
                0.0,
                1.0,
            ),
        ] {
            self.expect(
                number_in(number, minimum, maximum),
                field,
                "expected a finite number in the documented range",
            )?;
        }
        for field in ["attackFrames", "decayFrames", "releaseFrames"] {
            self.integer(
                &value["envelope"][field],
                &format!("$.envelope.{field}"),
                0,
                3600,
            )?;
        }
        self.integer(
            &value["pitch"]["vibratoPeriodFrames"],
            "$.pitch.vibratoPeriodFrames",
            0,
            3600,
        )?;
        let slide = value["pitch"]["slideSemitonesPerFrame"].as_f64();
        let depth = value["pitch"]["vibratoDepthSemitones"].as_f64();
        let release = value["envelope"]["releaseFrames"].as_f64().unwrap();
        self.expect(slide.zip(depth).is_some_and(|(slide, depth)| {
            let excursion = slide.abs() * (f64::from(u32::try_from(duration).unwrap()) + release) + depth.abs();
            excursion.is_finite() && (440.0 * 2_f64.powf((127.0 + excursion - 69.0) / 12.0)).is_finite()
        }), "$.pitch", "pitch excursion must remain finite for all MIDI notes and the full duration plus release")?;
        if value["waveform"] == "pulse" {
            self.expect(
                value["duty"]
                    .as_f64()
                    .is_some_and(|duty| duty > 0.0 && duty < 1.0),
                "$.duty",
                "pulse duty must be strictly between zero and one",
            )?;
        }
        if value["waveform"] == "wavetable" {
            let table = self.array(&value["wavetable"], "$.wavetable", 4, 32)?;
            self.expect(
                table.iter().all(|sample| number_in(sample, -1.0, 1.0)),
                "$.wavetable",
                "samples must be finite and in -1..=1",
            )?;
        }
        Ok(())
    }

    fn music(&self, value: &Value) -> Result<(), AssetDiagnostic> {
        self.integer(&value["framesPerRow"], "$.framesPerRow", 1, 240)?;
        self.expect(value["loop"].is_boolean(), "$.loop", "expected a boolean")?;
        let patterns = value["patterns"]
            .as_object()
            .ok_or_else(|| self.diagnostic("M014060", "$.patterns", "expected named patterns"))?;
        let order = self.array(&value["order"], "$.order", 1, MAX_ASSET_SOURCE_BYTES)?;
        for (index, name) in order.iter().enumerate() {
            self.expect(
                name.as_str()
                    .is_some_and(|name| patterns.contains_key(name)),
                &format!("$.order[{index}]"),
                "order entry must name an existing pattern",
            )?;
        }
        for (name, pattern) in patterns {
            let pattern_field =
                format!("$.patterns[{}].rows", serde_json::to_string(name).unwrap());
            let rows = self.array(&pattern["rows"], &pattern_field, 1, 256)?;
            for (row_index, row) in rows.iter().enumerate() {
                let row_field = format!("{pattern_field}[{row_index}]");
                let cells = self.array(row, &row_field, 8, 8)?;
                for (channel, cell) in cells.iter().enumerate() {
                    if cell.is_null() {
                        continue;
                    }
                    let field = format!("{row_field}[{channel}]");
                    self.integer(&cell["note"], &format!("{field}.note"), 0, 127)?;
                    self.expect(
                        cell["sound"]
                            .as_str()
                            .is_some_and(|sound| !sound.is_empty()),
                        &format!("{field}.sound"),
                        "expected a declared sound name",
                    )?;
                    self.expect(
                        cell.get("volume")
                            .is_none_or(|volume| number_in(volume, 0.0, 1.0)),
                        &format!("{field}.volume"),
                        "optional volume must be finite and in 0..=1",
                    )?;
                }
            }
        }
        Ok(())
    }

    fn display(&self, value: &Value) -> Result<Size, AssetDiagnostic> {
        self.expect(
            value.is_object() && value["kind"] == "display" && integer(&value["revision"], 2, 2),
            "$",
            "expected current display revision 2",
        )?;
        self.pixels(&value["remap"], "$.remap", 32, 255)?;
        let raster = self.array(&value["raster"], "$.raster", 0, 144)?;
        let mut previous = None;
        for (index, row) in raster.iter().enumerate() {
            let field = format!("$.raster[{index}]");
            let line = self.integer(&row["line"], &format!("{field}.line"), 0, 143)?;
            self.expect(
                previous.is_none_or(|previous| line > previous),
                &format!("{field}.line"),
                "raster lines must be unique and strictly increasing",
            )?;
            previous = Some(line);
            for key in ["scrollX", "scrollY"] {
                self.expect(
                    integer(&row[key], -32_768, 32_767),
                    &format!("{field}.{key}"),
                    "expected signed 16-bit scroll",
                )?;
            }
            self.pixels(&row["remap"], &format!("{field}.remap"), 32, 255)?;
        }
        Ok(Size {
            bytes: 32 + raster.len() * 38,
            allocations: 1 + raster.len(),
        })
    }
}

fn integer(value: &Value, minimum: i32, maximum: i32) -> bool {
    value.as_f64().is_some_and(|value| {
        value.fract() == 0.0 && value >= f64::from(minimum) && value <= f64::from(maximum)
    })
}

fn number_in(value: &Value, minimum: f64, maximum: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|value| value.is_finite() && (minimum..=maximum).contains(&value))
}

/// The same per-document schema used by admission, without aggregate capacity checks.
pub(crate) fn validate_asset_document(
    name: &str,
    path: &str,
    kind: AssetKind,
    value: &Value,
) -> Result<(), AssetDiagnostic> {
    Schema { name, path }.validate(kind, value).map(|_| ())
}

/// Reports all independent asset failures (bounded to 128) without changing authored files.
/// Usage includes only schema-valid payloads; any diagnostic means the bank is not loadable.
#[must_use]
pub fn analyze_project_assets(
    manifest: &ProjectManifest,
    files: &BTreeMap<String, Vec<u8>>,
) -> AssetReport {
    let mut report = AssetReport::default();
    if manifest.assets.len() > 4096 {
        report.push(AssetDiagnostic {
            code: "M014062",
            asset: "BANK".to_owned(),
            path: "cart.m01w".to_owned(),
            field: "$.assets".to_owned(),
            message: format!(
                "{} asset declarations exceed the 4096 limit",
                manifest.assets.len()
            ),
        });
        return report;
    }
    let mut valid = BTreeMap::new();
    let mut parsed = BTreeMap::<&str, Arc<Value>>::new();
    let paths = manifest
        .assets
        .values()
        .map(|asset| asset.path.as_str())
        .chain(manifest.display.as_deref())
        .collect::<BTreeSet<_>>();
    report.usage.source_bytes = paths
        .iter()
        .filter_map(|path| files.get(*path))
        .map(Vec::len)
        .sum();
    if report.usage.source_bytes > MAX_ASSET_SOURCE_BYTES {
        report.push(AssetDiagnostic {
            code: "M014062",
            asset: "BANK".to_owned(),
            path: "cart.m01w".to_owned(),
            field: "$.assets".to_owned(),
            message: format!(
                "asset sources use {} bytes; limit is {MAX_ASSET_SOURCE_BYTES}",
                report.usage.source_bytes
            ),
        });
        return report;
    }
    for (name, asset) in &manifest.assets {
        let schema = Schema {
            name,
            path: &asset.path,
        };
        let Some(bytes) = files.get(&asset.path) else {
            report.push(schema.diagnostic("M014063", "$", "asset file is missing"));
            continue;
        };
        let value = if let Some(value) = parsed.get(asset.path.as_str()) {
            Arc::clone(value)
        } else {
            match serde_json::from_slice(bytes) {
                Ok(value) => {
                    let value = Arc::new(value);
                    parsed.insert(&asset.path, Arc::clone(&value));
                    value
                }
                Err(error) => {
                    report.push(schema.diagnostic(
                        "M014060",
                        "$",
                        format!("invalid UTF-8 JSON: {error}"),
                    ));
                    continue;
                }
            }
        };
        match schema.validate(asset.kind, &value) {
            Ok(size) => {
                if matches!(asset.kind, AssetKind::Sound | AssetKind::Music) {
                    report.usage.audio_assets += 1;
                } else {
                    report.usage.visual_assets += 1;
                    report.usage.visual_bytes += size.bytes;
                    report.usage.visual_allocations += size.allocations;
                }
                valid.insert(name.clone(), value);
            }
            Err(diagnostic) => report.push(diagnostic),
        }
    }
    validate_references(manifest, &valid, &mut report);
    analyze_display(manifest, files, &mut report);
    capacity_diagnostics(&mut report);
    report
}

fn validate_references(
    manifest: &ProjectManifest,
    valid: &BTreeMap<String, Arc<Value>>,
    report: &mut AssetReport,
) {
    for (name, value) in valid {
        let asset = &manifest.assets[name];
        let schema = Schema {
            name,
            path: &asset.path,
        };
        if asset.kind == AssetKind::Map {
            for (index, layer) in value["layers"].as_array().unwrap().iter().enumerate() {
                let target = layer["tileSet"].as_str().unwrap();
                let field = format!("$.layers[{index}].tileSet");
                if manifest
                    .assets
                    .get(target)
                    .is_none_or(|asset| asset.kind != AssetKind::TileSet)
                {
                    report.push(schema.diagnostic(
                        "M014061",
                        &field,
                        format!("'{target}' must name a declared tile_set"),
                    ));
                } else if let Some(tiles) = valid
                    .get(target)
                    .and_then(|tile_set| tile_set["tiles"].as_array())
                    && let Some(cell) = layer["cells"].as_array().unwrap().iter().position(|cell| {
                        cell.as_f64().unwrap() >= f64::from(u32::try_from(tiles.len()).unwrap())
                    })
                {
                    report.push(schema.diagnostic(
                        "M014061",
                        &format!("$.layers[{index}].cells[{cell}]"),
                        format!(
                            "tile {} is outside tile set '{target}' ({} tiles)",
                            layer["cells"][cell],
                            tiles.len()
                        ),
                    ));
                }
            }
        } else if asset.kind == AssetKind::Music {
            let mut missing = BTreeSet::new();
            for (pattern_name, pattern) in value["patterns"].as_object().unwrap() {
                for (row, cells) in pattern["rows"].as_array().unwrap().iter().enumerate() {
                    for (channel, cell) in cells.as_array().unwrap().iter().enumerate() {
                        let Some(sound) = cell["sound"].as_str() else {
                            continue;
                        };
                        if manifest
                            .assets
                            .get(sound)
                            .is_none_or(|asset| asset.kind != AssetKind::Sound)
                            && missing.insert(sound)
                        {
                            report.push(schema.diagnostic(
                                "M014061",
                                &format!(
                                    "$.patterns[{}].rows[{row}][{channel}].sound",
                                    serde_json::to_string(pattern_name).unwrap()
                                ),
                                format!("'{sound}' must name a declared sound"),
                            ));
                        }
                    }
                }
            }
        }
    }
}

fn analyze_display(
    manifest: &ProjectManifest,
    files: &BTreeMap<String, Vec<u8>>,
    report: &mut AssetReport,
) {
    if let Some(path) = &manifest.display {
        let schema = Schema {
            name: "DISPLAY",
            path,
        };
        let decoded = files
            .get(path)
            .ok_or_else(|| schema.diagnostic("M014063", "$", "display file is missing"))
            .and_then(|bytes| {
                serde_json::from_slice(bytes).map_err(|error| {
                    schema.diagnostic("M014060", "$", format!("invalid UTF-8 JSON: {error}"))
                })
            })
            .and_then(|value| schema.display(&value));
        match decoded {
            Ok(size) => {
                report.usage.visual_bytes += size.bytes;
                report.usage.visual_allocations += size.allocations;
            }
            Err(diagnostic) => report.push(diagnostic),
        }
    }
}

fn capacity_diagnostics(report: &mut AssetReport) {
    for (field, used, maximum) in [
        (
            "visualBytes",
            report.usage.visual_bytes,
            VISUAL_CAPACITY_BYTES,
        ),
        (
            "visualAllocations",
            report.usage.visual_allocations,
            VISUAL_ALLOCATION_CAPACITY,
        ),
        ("visualAssets", report.usage.visual_assets, 4096),
    ] {
        if used > maximum {
            report.push(AssetDiagnostic {
                code: "M014062",
                asset: "BANK".to_owned(),
                path: "cart.m01w".to_owned(),
                field: format!("$.{field}"),
                message: format!(
                    "{field} uses {used}; limit {maximum}; over by {}",
                    used - maximum
                ),
            });
        }
    }
}

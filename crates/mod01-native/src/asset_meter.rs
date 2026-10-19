//! Read-only, debounced view of the shared asset admission report, including unsaved editor data.
use crate::{project::NativeProject, ui_theme};
use eframe::egui;
use modl_core::{
    AssetKind, ProjectAsset,
    assets::{
        AssetReport, VISUAL_ALLOCATION_CAPACITY, VISUAL_CAPACITY_BYTES, analyze_project_assets,
    },
};
use std::time::{Duration, Instant};

const REFRESH_INTERVAL: Duration = Duration::from_millis(250);

pub struct Overlay {
    pub path: String,
    pub source: Vec<u8>,
    pub declaration: Option<(String, AssetKind)>,
}

#[derive(Default)]
pub struct AssetMeter {
    pub report: AssetReport,
    updated: Option<Instant>,
}

impl AssetMeter {
    pub fn due(&self, now: Instant) -> bool {
        self.updated
            .is_none_or(|updated| now.saturating_duration_since(updated) >= REFRESH_INTERVAL)
    }

    pub fn refresh(&mut self, project: &NativeProject, overlay: Option<Overlay>, now: Instant) {
        self.report = analyze(project, overlay);
        self.updated = Some(now);
    }

    /// Returns the asset selected through a diagnostic, if any.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut selected = None;
        let usage = &self.report.usage;
        let valid = self.report.is_valid();
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                if valid {
                    ui_theme::CYAN
                } else {
                    ui_theme::CORAL
                },
                if valid {
                    "ASSET BANK / VALID"
                } else {
                    "ASSET BANK / INVALID (PARTIAL USAGE)"
                },
            );
            if ui.small_button("REFRESH").clicked() {
                self.updated = None;
            }
        });
        ui.label(format!(
            "VISUAL {} / {} B  ·  SLOTS {} / {}  ·  AUDIO {}",
            usage.visual_bytes,
            VISUAL_CAPACITY_BYTES,
            usage.visual_allocations,
            VISUAL_ALLOCATION_CAPACITY,
            usage.audio_assets
        ));
        ui.add(
            egui::ProgressBar::new(usage.visual_bytes as f32 / VISUAL_CAPACITY_BYTES as f32)
                .desired_width(ui.available_width())
                .desired_height(8.0)
                .corner_radius(0),
        );
        if !valid {
            egui::CollapsingHeader::new(format!(
                "{} ASSET DIAGNOSTICS",
                self.report.diagnostics.len()
            ))
            .id_salt("asset-diagnostics-header")
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("asset-diagnostics-list")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for diagnostic in &self.report.diagnostics {
                            ui.horizontal_wrapped(|ui| {
                                if ui.small_button(&diagnostic.asset).clicked() {
                                    selected = Some(diagnostic.asset.clone());
                                }
                                ui.label(format!(
                                    "{} · {}:{} · {}",
                                    diagnostic.code,
                                    diagnostic.path,
                                    diagnostic.field,
                                    diagnostic.message
                                ));
                            });
                        }
                        if self.report.diagnostics_truncated {
                            ui.label("FURTHER DIAGNOSTICS SUPPRESSED / LIMIT 128");
                        }
                    });
            });
        }
        ui.ctx().request_repaint_after(REFRESH_INTERVAL);
        selected
    }
}

pub fn analyze(project: &NativeProject, overlay: Option<Overlay>) -> AssetReport {
    let Some(overlay) = overlay else {
        return analyze_project_assets(&project.manifest, &project.files);
    };
    let mut files = project.files.clone();
    files.insert(overlay.path.clone(), overlay.source);
    let mut manifest = project.manifest.clone();
    if let Some((name, kind)) = overlay.declaration {
        manifest.assets.insert(
            name,
            ProjectAsset {
                kind,
                path: overlay.path,
            },
        );
    }
    analyze_project_assets(&manifest, &files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn unsaved_music_errors_and_sprite_frame_costs_are_reported_without_mutating_project_or_files()
    {
        let project = NativeProject::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/api-tour/cart.m01w"),
        )
        .unwrap();
        let original_files = project.files.clone();
        let original_manifest = project.manifest.clone();
        let mut song: Value = serde_json::from_slice(&project.files["assets/loop.m01t"]).unwrap();
        song["patterns"]["A"]["rows"][0][0]["sound"] = json!("missing_patch");
        let report = analyze(
            &project,
            Some(Overlay {
                path: "assets/loop.m01t".to_owned(),
                source: serde_json::to_vec(&song).unwrap(),
                declaration: None,
            }),
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.asset == "loop" && diagnostic.code == "M014061")
        );
        let base = analyze(&project, None);
        let mut sprite: Value = serde_json::from_slice(&project.files["assets/dot.m01g"]).unwrap();
        let bytes = sprite["width"].as_u64().unwrap() * sprite["height"].as_u64().unwrap();
        let frame = sprite["frames"][0].clone();
        sprite["frames"].as_array_mut().unwrap().push(frame);
        let report = analyze(
            &project,
            Some(Overlay {
                path: "assets/dot.m01g".to_owned(),
                source: serde_json::to_vec(&sprite).unwrap(),
                declaration: Some(("dot".to_owned(), AssetKind::Animation)),
            }),
        );
        assert!(report.is_valid(), "{:?}", report.diagnostics);
        assert_eq!(
            report.usage.visual_bytes,
            base.usage.visual_bytes + bytes as usize
        );
        assert_eq!(
            report.usage.visual_allocations,
            base.usage.visual_allocations + 1
        );
        assert_eq!(project.files, original_files);
        assert_eq!(project.manifest, original_manifest);
        assert!(!project.dirty);
    }

    #[test]
    fn invalid_raw_json_is_visible_and_refreshes_are_clock_bounded() {
        let project = NativeProject::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/api-tour/cart.m01w"),
        )
        .unwrap();
        let mut meter = AssetMeter::default();
        let now = Instant::now();
        assert!(meter.due(now));
        meter.refresh(
            &project,
            Some(Overlay {
                path: "assets/loop.m01t".to_owned(),
                source: b"invalid".to_vec(),
                declaration: None,
            }),
            now,
        );
        assert_eq!(meter.report.diagnostics[0].asset, "loop");
        assert_eq!(meter.report.diagnostics[0].code, "M014060");
        assert!(!meter.due(now + Duration::from_millis(249)));
        assert!(meter.due(now + REFRESH_INTERVAL));
    }
}

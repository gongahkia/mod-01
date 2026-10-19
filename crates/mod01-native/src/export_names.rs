//! Portable suggestions for asset save dialogs, not asset identities or paths.
use std::path::Path;

/// Keep a readable, bounded ASCII label behind a non-device namespace. The user
/// still chooses the destination and may change the suggested name in the dialog.
pub fn asset(name: &str, extension: &str) -> String {
    let mut label: String = name
        .chars()
        .take(64)
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if label.is_empty() {
        label.push_str("unnamed");
    }
    let extension = if !extension.is_empty()
        && extension.len() <= 16
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        extension
    } else {
        "json"
    };
    format!("asset-{label}.{extension}")
}

pub fn source(name: &str, path: &str) -> String {
    asset(
        name,
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("json"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_asset_export_formats_get_bounded_non_device_single_component_names() {
        for name in [
            "CON",
            "con",
            "NUL",
            "PRN",
            "AUX",
            "COM1",
            "LPT9",
            "_hero",
            "Hero_2",
            "",
            "../CON.",
            "a/b\\c:d*e?f\"g<h>i|j",
            "é🦀\0\n",
        ]
        .into_iter()
        .chain(["a".repeat(1024).as_str()])
        {
            for extension in [
                "png", "wav", "m01g", "m01m", "m01f", "m01s", "m01t", "m01p", "json",
            ] {
                let filename = asset(name, extension);
                assert!(filename.starts_with("asset-"));
                assert!(filename.ends_with(&format!(".{extension}")));
                assert!(filename.len() <= 6 + 64 + 1 + 16);
                assert_eq!(Path::new(&filename).components().count(), 1);
                assert!(
                    filename
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric()
                            || matches!(byte, b'_' | b'-' | b'.'))
                );
                assert_eq!(filename.matches('.').count(), 1);
            }
        }
        assert_eq!(asset("CON", "png"), "asset-CON.png");
        assert_eq!(asset("Hero_2", "wav"), "asset-Hero_2.wav");
    }

    #[test]
    fn raw_exports_keep_safe_source_extensions_without_using_the_declared_basename() {
        assert_eq!(source("beep", "assets/CON.m01s"), "asset-beep.m01s");
        assert_eq!(source("DISPLAY", "display.m01p"), "asset-DISPLAY.m01p");
        assert_eq!(source("font", "assets/typeface"), "asset-font.json");
        for extension in ["", "../png", "png ", "x:y", "é", &"p".repeat(100)] {
            assert_eq!(asset("CON", extension), "asset-CON.json");
        }
    }
}

//! Data shared by the compiler's native tools and the production console.
use std::sync::OnceLock;

/// Immutable RGB values for all 256 master-palette indices.
/// The exact same checked-in data is bundled by the TypeScript console runtime.
///
/// # Panics
/// Panics only if the checked-in palette is malformed or has the wrong size.
/// Repository tests validate that invariant; no cartridge data is parsed here.
#[must_use]
pub fn master_palette() -> &'static [[u8; 3]; 256] {
    static PALETTE: OnceLock<[[u8; 3]; 256]> = OnceLock::new();
    PALETTE.get_or_init(|| {
        let entries: Vec<[u8; 3]> =
            serde_json::from_str(include_str!("../../../packages/runtime/src/palette.json"))
                .expect("built-in palette is valid RGB data");
        entries
            .try_into()
            .expect("built-in palette has 256 colours")
    })
}

#[cfg(test)]
mod tests {
    use super::master_palette;
    use std::collections::BTreeSet;

    #[test]
    fn all_native_tools_use_the_complete_unique_console_palette() {
        let palette = master_palette();
        assert_eq!(palette.iter().collect::<BTreeSet<_>>().len(), 256);
        assert_eq!(palette[0], [23, 20, 31]);
        assert_eq!(palette[25], [52, 85, 129]);
        assert_eq!(palette[32], [0, 0, 0]);
        assert_eq!(palette[247], [255, 255, 255]);
        assert_eq!(palette[255], [229, 229, 229]);
    }
}

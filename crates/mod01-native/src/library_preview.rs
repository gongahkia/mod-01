//! Bounded, offline cover rendering. No cartridge code is executed to browse a library.
use crate::project::NativeProject;
use eframe::egui::ColorImage;
use resvg::{tiny_skia, usvg};
use std::{io::Cursor, path::Path};

const MAX_ART: usize = 256 * 1024;
const WIDTH: u32 = 240;
const HEIGHT: u32 = 144;

pub struct Preview {
    pub title: String,
    pub id: String,
    pub author: String,
    pub image: Option<ColorImage>,
    pub warning: Option<String>,
}

pub fn load(path: &Path, art: bool) -> Result<Preview, String> {
    let file = if path.is_dir() {
        path.join("cart.m01w")
    } else {
        path.to_path_buf()
    };
    if !file.is_file() {
        return Err("cartridge is missing or is not an ordinary file".to_owned());
    }
    let project = NativeProject::open(path)?;
    let mut preview = Preview {
        title: bounded(&project.manifest.title, 160),
        id: bounded(&project.manifest.id, 100),
        author: bounded(&project.manifest.author, 100),
        image: None,
        warning: None,
    };
    if art {
        let cover = project
            .manifest
            .label
            .as_ref()
            .or(project.manifest.thumbnail.as_ref());
        if let Some(name) = cover {
            match project
                .files
                .get(name)
                .ok_or_else(|| "cover file is missing".to_owned())
                .and_then(|bytes| render(bytes))
            {
                Ok(image) => preview.image = Some(image),
                Err(error) => preview.warning = Some(format!("COVER UNAVAILABLE: {error}")),
            }
        }
    }
    Ok(preview)
}

fn bounded(value: &str, length: usize) -> String {
    value
        .chars()
        .take(length)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn render(bytes: &[u8]) -> Result<ColorImage, String> {
    if bytes.len() > MAX_ART {
        return Err("cover exceeds 256 KiB".to_owned());
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut reader =
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(1024);
        limits.max_image_height = Some(1024);
        limits.max_alloc = Some(8 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader
            .decode()
            .map_err(|error| error.to_string())?
            .resize(WIDTH, HEIGHT, image::imageops::FilterType::Nearest)
            .into_rgba8();
        return Ok(ColorImage::from_rgba_unmultiplied(
            [decoded.width() as usize, decoded.height() as usize],
            decoded.as_raw(),
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "cover needs SVG or PNG".to_owned())?;
    let xml = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 4096,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    if xml
        .descendants()
        .filter(roxmltree::Node::is_text)
        .map(|node| node.text().unwrap_or_default().chars().count())
        .sum::<usize>()
        > 8192
    {
        return Err("SVG text exceeds 8192 characters".to_owned());
    }
    for node in xml.descendants().filter(roxmltree::Node::is_element) {
        // Avoid reusable subtrees, filter intermediates, raster decoders and tiny repeating
        // patterns. Source is retained untouched; unsupported art never prevents launch.
        if !matches!(
            node.tag_name().name(),
            "svg"
                | "g"
                | "defs"
                | "title"
                | "desc"
                | "rect"
                | "circle"
                | "ellipse"
                | "line"
                | "polyline"
                | "polygon"
                | "path"
                | "text"
                | "tspan"
                | "clipPath"
                | "linearGradient"
                | "radialGradient"
                | "stop"
        ) || node.ancestors().count() > 32
        {
            return Err("SVG preview supports bounded static shapes and text only".to_owned());
        }
        if node.attributes().any(|attribute| {
            matches!(attribute.name(), "href" | "filter" | "mask")
                || attribute.name().starts_with("on")
                || attribute.value().len() > 16 * 1024
        }) {
            return Err("external references, filters and scripts are disabled".to_owned());
        }
    }
    let mut options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    // Only our shipped font. No system/font-directory lookup, URLs or external resources.
    let database = options.fontdb_mut();
    database.load_font_data(crate::ui_assets::BIG_BLUE.to_vec());
    let family = database
        .faces()
        .next()
        .and_then(|face| face.families.first())
        .map(|(family, _)| family.clone())
        .ok_or("bundled cover font is unavailable")?;
    database.set_monospace_family(&family);
    database.set_sans_serif_family(&family);
    database.set_serif_family(&family);
    options.font_family = family;
    let tree = usvg::Tree::from_str(text, &options).map_err(|error| error.to_string())?;
    let size = tree.size();
    if size.width() > 4096.0 || size.height() > 4096.0 {
        return Err("SVG viewport exceeds 4096 pixels".to_owned());
    }
    let scale = (WIDTH as f32 / size.width()).min(HEIGHT as f32 / size.height());
    let mut pixels =
        tiny_skia::Pixmap::new(WIDTH, HEIGHT).ok_or("cannot allocate cover preview")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale).post_translate(
            (WIDTH as f32 - size.width() * scale) / 2.0,
            (HEIGHT as f32 - size.height() * scale) / 2.0,
        ),
        &mut pixels.as_mut(),
    );
    // tiny-skia's bytes are premultiplied; preserve alpha rather than multiplying it twice.
    Ok(ColorImage::new(
        [WIDTH as usize, HEIGHT as usize],
        pixels
            .pixels()
            .iter()
            .map(|pixel| {
                eframe::egui::Color32::from_rgba_premultiplied(
                    pixel.red(),
                    pixel.green(),
                    pixel.blue(),
                    pixel.alpha(),
                )
            })
            .collect(),
    ))
}

/// Catch malformed third-party cover/parser panics at the worker boundary, not the UI thread.
pub fn guarded_load(path: &Path, art: bool) -> Result<Preview, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load(path, art)))
        .unwrap_or_else(|_| Err("cartridge preview failed; source has not been changed".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintained_covers_render_with_real_bundled_text_and_without_runtime_execution() {
        let mut rendered = 0;
        for path in crate::bundled_samples() {
            let preview = load(&path, true).unwrap();
            assert!(!preview.title.is_empty());
            assert!(
                preview.warning.is_none(),
                "{}: {:?}",
                path.display(),
                preview.warning
            );
            let Some(image) = preview.image else {
                continue;
            };
            rendered += 1;
            assert_eq!(image.size, [240, 144]);
            assert!(image.pixels.iter().any(|pixel| pixel.a() != 0));
        }
        assert!(
            rendered >= 7,
            "maintained authored cover art is actually rendered"
        );
        let transparent = render(br#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="144"><rect width="240" height="144" fill="red" opacity="0.5"/></svg>"#).unwrap();
        assert!((126..=129).contains(&transparent.pixels[0].a()));
        assert!((126..=129).contains(&transparent.pixels[0].r()));
        let text = render(br#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="144"><text x="10" y="30" font-family="monospace" fill="white">REAL TEXT</text></svg>"#).unwrap();
        assert!(
            text.pixels.iter().any(|pixel| pixel.a() != 0),
            "font rendered actual glyphs"
        );
    }

    #[test]
    fn external_resources_expansion_filters_and_oversized_previews_are_rejected() {
        for body in [
            "<image href=\"file:///etc/passwd\"/>",
            "<image href=\"https://example.com/a.png\"/>",
            "<image href=\"data:image/svg+xml;base64,\"/>",
            "<use href=\"#nested\"/>",
            "<filter id=\"f\"/>",
            "<script>bad()</script>",
            "<pattern id=\"p\"/>",
            "<rect width=\"1\" height=\"1\" onclick=\"bad()\"/>",
        ] {
            assert!(
                render(
                    format!("<svg xmlns=\"http://www.w3.org/2000/svg\">{body}</svg>").as_bytes()
                )
                .is_err()
            );
        }
        assert!(render(b"<!DOCTYPE svg [<!ENTITY x 'expanded'>]><svg>&x;</svg>").is_err());
        assert!(render(&vec![b' '; MAX_ART + 1]).is_err());
        assert!(render(b"<svg width=\"1000000\" height=\"1000000\"/>").is_err());
        assert!(render(format!("<svg>{}</svg>", "<g/>".repeat(4096)).as_bytes()).is_err());
        // PNG limits are checked before allocating a decompressed image.
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1025, 1)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert!(render(png.get_ref()).is_err());
    }
}

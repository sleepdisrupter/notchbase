//! The Virtual Display Driver's settings file, which lists the sizes it offers. Kept apart from
//! the Windows code so it's tested everywhere.

/// The settings with `width` by `height` among the resolutions, or `None` when it's there
/// already (or the file has no list of resolutions).
pub fn with_resolution(text: &str, [width, height]: [u32; 2], hz: u32) -> Option<String> {
    let start = text.find("<resolutions>")?;
    let end = start + text[start..].find("</resolutions>")?;
    let listed = &text[start..end];
    let wanted_w = format!("<width>{width}</width>");
    let wanted_h = format!("<height>{height}</height>");
    let present = listed.split("<resolution>").skip(1).any(|block| {
        let block = block.split("</resolution>").next().unwrap_or("");
        let squeezed: String = block.chars().filter(|c| !c.is_whitespace()).collect();
        squeezed.contains(&wanted_w) && squeezed.contains(&wanted_h)
    });
    if present {
        return None;
    }
    let entry = format!(
        "    <resolution>\n            <width>{width}</width>\n            <height>{height}</height>\n            <refresh_rate>{hz}</refresh_rate>\n        </resolution>\n    "
    );
    Some(format!("{}{entry}{}", &text[..end], &text[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: &str = "<vdd_settings>\n    <resolutions>\n        <resolution>\n            <width>1920</width>\n            <height>1080</height>\n            <refresh_rate>60</refresh_rate>\n        </resolution>\n    </resolutions>\n</vdd_settings>\n";

    #[test]
    fn a_missing_size_is_added_once() {
        let added = with_resolution(SETTINGS, [2560, 1440], 60).unwrap();
        assert!(added.contains("<width>2560</width>") && added.contains("<height>1440</height>"));
        assert_eq!(added.matches("<resolution>").count(), 2);
        assert!(added.trim_end().ends_with("</resolutions>\n</vdd_settings>"));
        assert_eq!(with_resolution(&added, [2560, 1440], 60), None, "already there");
    }

    #[test]
    fn a_listed_size_changes_nothing() {
        assert_eq!(with_resolution(SETTINGS, [1920, 1080], 60), None);
        // The width alone isn't enough.
        assert!(with_resolution(SETTINGS, [1920, 1200], 60).is_some());
        assert_eq!(with_resolution("<vdd_settings/>", [1, 1], 60), None);
    }
}

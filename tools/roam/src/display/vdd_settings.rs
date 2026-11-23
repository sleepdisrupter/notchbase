//! Reading the Virtual Display Driver's settings file (`vdd_settings.xml`): which sizes its
//! screens offer. Changing it takes administrator rights and a restart of the driver, which
//! `windows/vdd-room.ps1` does.

/// The sizes the settings list, in the order they're listed.
pub fn sizes(xml: &str) -> Vec<[u32; 2]> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<resolution>") {
        rest = &rest[start + "<resolution>".len()..];
        let end = rest.find("</resolution>").unwrap_or(rest.len());
        let block = &rest[..end];
        if let (Some(w), Some(h)) = (number(block, "width"), number(block, "height"))
            && !found.contains(&[w, h])
        {
            found.push([w, h]);
        }
        rest = &rest[end..];
    }
    found
}

/// The number in `<tag>…</tag>` within `block`.
fn number(block: &str, tag: &str) -> Option<u32> {
    let open = format!("<{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find('<')? + start;
    block[start..end].trim().parse().ok()
}

/// What to pass the script for the sizes in `wanted` the settings don't list: `2880x1620,…`.
pub fn missing(listed: &[[u32; 2]], wanted: &[[u32; 2]]) -> String {
    let mut out: Vec<[u32; 2]> = Vec::new();
    for &size in wanted {
        let [w, h] = size;
        let fits =
            w >= 640 && h >= 480 && w % 2 == 0 && h % 2 == 0 && w <= super::MOST_UNITS[0] && h <= super::MOST_UNITS[1];
        if fits && !listed.contains(&size) && !out.contains(&size) {
            out.push(size);
        }
    }
    out.iter().map(|[w, h]| format!("{w}x{h}")).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = "<vdd_settings>\n    <monitors>\n        <count>1</count>\n    </monitors>\n    <resolutions>\n        <resolution>\n            <width>800</width>\n            <height>600</height>\n            <refresh_rate>30</refresh_rate>\n        </resolution>\n        <resolution>\n            <width>1920</width>\n            <height> 1080 </height>\n            <refresh_rate>30</refresh_rate>\n        </resolution>\n        <resolution><width>bad</width><height>1</height></resolution>\n    </resolutions>\n</vdd_settings>\n";

    #[test]
    fn the_driver_lists_its_sizes() {
        assert_eq!(sizes(SHIPPED), vec![[800, 600], [1920, 1080]]);
        assert_eq!(sizes(""), Vec::<[u32; 2]>::new());
    }

    #[test]
    fn only_sizes_it_lacks_are_added() {
        let listed = sizes(SHIPPED);
        assert_eq!(missing(&listed, &[[2880, 1620], [1920, 1080], [2880, 1620], [2206, 1434]]), "2880x1620,2206x1434");
        assert_eq!(missing(&listed, &[[1920, 1080]]), "");
        assert_eq!(
            missing(&listed, &[[2881, 1620], [100, 100], [9000, 5000]]),
            "",
            "odd, tiny and huge sizes stay out"
        );
    }
}

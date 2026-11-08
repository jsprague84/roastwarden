//! Roastwarden's mark, "Warden" (docs/ui-structure.md, "Visual direction"): the SVGs in
//! `assets/brand/`, as the server puts them on its share pages and in its reports' footers.
//! The dashboard carries the same drawing in `Mark.svelte` and `static/favicon.svg`.

use std::collections::BTreeMap;

use serde_bytes::ByteBuf;

/// The mark in one colour (`currentColor`), as drawn.
const MARK_MONO: &str = include_str!("../../assets/brand/mark-mono.svg");

/// The file a report design loads the mark as (`image("brand/mark.svg", …)`).
pub const REPORT_FILE: &str = "brand/mark.svg";

/// The colour the reports' footer text is set in (`soft` in `assets/reports/report.typ`).
const REPORT_INK: &str = "#616161";

/// The drawing without its comment: what goes inline.
fn drawing() -> String {
    let mut s = String::with_capacity(MARK_MONO.len());
    let mut rest = MARK_MONO;
    while let Some(start) = rest.find("<!--") {
        s.push_str(&rest[..start]);
        rest = match rest[start..].find("-->") {
            Some(end) => &rest[start + end + 3..],
            None => "",
        };
    }
    s.push_str(rest);
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The one-colour mark inline in HTML, `px` square, coloured by the text around it.
pub fn html(px: u32) -> String {
    drawing().replacen(
        "<svg ",
        &format!(r#"<svg class="mk" width="{px}" height="{px}" aria-hidden="true" "#),
        1,
    )
}

/// The one-colour mark as the file report designs load (`REPORT_FILE`), in the footer's ink.
pub fn report_files() -> BTreeMap<String, ByteBuf> {
    BTreeMap::from([(
        REPORT_FILE.to_string(),
        ByteBuf::from(drawing().replace("currentColor", REPORT_INK).into_bytes()),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inline_mark_is_one_svg_in_the_text_colour() {
        let h = html(14);
        assert!(h.starts_with(r#"<svg class="mk" width="14" height="14" aria-hidden="true""#));
        assert!(h.ends_with("</svg>"));
        assert!(!h.contains("<!--"), "{h}");
        assert_eq!(h.matches("currentColor").count(), 2, "shield and flame");
    }

    #[test]
    fn the_report_mark_is_set_in_the_footer_ink() {
        let files = report_files();
        let svg = std::str::from_utf8(&files[REPORT_FILE]).unwrap();
        assert!(!svg.contains("currentColor"));
        assert_eq!(svg.matches(REPORT_INK).count(), 2);
    }
}

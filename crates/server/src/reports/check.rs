//! The report check (`docs/reports.md`, "The report check"): a report's pages
//! read back as text, and what a reader would call a layout fault. Every kind's tests run it
//! on their fixtures (a test below makes sure each kind's file does).
//!
//! - **An orphan last page**: a document of two pages or more whose last page holds less
//!   than [`ORPHAN_MM`] of content (a closing note, the end of a table): it belongs on the
//!   page before.
//! - **Colliding text**: two runs of text whose ink overlaps by more than [`OVERLAP_MM`]
//!   both ways (event names on a chart, end labels, rings on a strip, tick labels).
//! - **Text past the paper**: ink outside the page.

use serde_json::Value;

use crate::labels::engine::{self, PageText, Request};

/// The least content a last page should hold, mm (top of its first line to the bottom of its
/// last, header and footer apart)
pub const ORPHAN_MM: f64 = 20.0;
/// More than this both ways is a collision, mm (as the label check)
pub const OVERLAP_MM: f64 = 0.2;

/// The header and footer bands, mm from the top and bottom of the page: `page-frame`'s
/// margins (20 mm, 18 mm) less a little
const HEADER_MM: f64 = 16.0;
const FOOTER_MM: f64 = 14.0;

fn pt(mm: f64) -> f64 {
    mm * 72.0 / 25.4
}

fn mm(pt: f64) -> f64 {
    (pt * 25.4 / 72.0 * 10.0).round() / 10.0
}

/// A run of text joined with the runs it abuts on its baseline (Typst sets a line in pieces):
/// (text, [x0, y0, x1, y1])
type Piece = (String, [f64; 4]);

fn pieces(page: &PageText) -> Vec<Piece> {
    let mut out: Vec<(Piece, f64, f64)> = Vec::new();
    for (text, size, base, b) in &page.runs {
        if text.trim().is_empty() {
            continue;
        }
        if let Some(((t, last), lbase, lsize)) = out.last_mut() {
            let gap = b[0] - last[2];
            // Abutting on one baseline (a space or a change of face between them)
            if (*lbase - base).abs() < 0.3 * size.max(*lsize) && gap > -0.3 && gap < 0.6 * size {
                if gap > 0.12 * size {
                    t.push(' ');
                }
                t.push_str(text);
                *last = [
                    last[0].min(b[0]),
                    last[1].min(b[1]),
                    last[2].max(b[2]),
                    last[3].max(b[3]),
                ];
                continue;
            }
        }
        out.push(((text.clone(), *b), *base, *size));
    }
    out.into_iter().map(|(p, _, _)| p).collect()
}

fn short(t: &str) -> String {
    let t = t.trim();
    let mut s: String = t.chars().take(30).collect();
    if t.chars().count() > 30 {
        s.push('…');
    }
    format!("\"{s}\"")
}

/// What is wrong with these pages, worst first; empty when nothing is.
pub fn faults(pages: &[PageText]) -> Vec<String> {
    let mut out = Vec::new();
    for (k, page) in pages.iter().enumerate() {
        let n = k + 1;
        let list = pieces(page);
        for (t, b) in &list {
            if b[0] < -0.5 || b[1] < -0.5 || b[2] > page.w + 0.5 || b[3] > page.h + 0.5 {
                out.push(format!("page {n}: {} runs past the paper", short(t)));
            }
        }
        for (i, (ta, a)) in list.iter().enumerate() {
            for (tb, b) in &list[i + 1..] {
                let ox = a[2].min(b[2]) - a[0].max(b[0]);
                let oy = a[3].min(b[3]) - a[1].max(b[1]);
                if ox > pt(OVERLAP_MM) && oy > pt(OVERLAP_MM) {
                    out.push(format!(
                        "page {n}: {} and {} collide ({} × {} mm) at {}, {} mm",
                        short(ta),
                        short(tb),
                        mm(ox),
                        mm(oy),
                        mm(a[0].max(b[0])),
                        mm(a[1].max(b[1]))
                    ));
                }
            }
        }
    }
    if pages.len() >= 2 {
        let last = pages.last().expect("two pages");
        let body: Vec<Piece> = pieces(last)
            .into_iter()
            .filter(|(_, b)| b[1] > pt(HEADER_MM) && b[3] < last.h - pt(FOOTER_MM))
            .collect();
        let top = body.iter().map(|p| p.1[1]).fold(f64::INFINITY, f64::min);
        let bottom = body
            .iter()
            .map(|p| p.1[3])
            .fold(f64::NEG_INFINITY, f64::max);
        let tall = if body.is_empty() { 0.0 } else { bottom - top };
        if tall < pt(ORPHAN_MM) {
            out.insert(
                0,
                format!(
                    "page {} of {} holds only {} mm: {}",
                    pages.len(),
                    pages.len(),
                    mm(tall),
                    body.iter()
                        .map(|p| short(&p.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
    }
    out
}

/// Lay a kind's data out in its design and check it.
pub fn layout(kind: &str, data: &Value) -> Vec<String> {
    let k = super::kind(kind).expect("a kind of ours");
    layout_design(k.design, data)
}

/// Lay a design's source out on this data and check it.
pub fn layout_design(design: &str, data: &Value) -> Vec<String> {
    let kind = "design";
    let req = Request {
        kind: engine::Kind::Report,
        design: Some(design.to_string()),
        more: vec![],
        data: data.clone(),
        files: crate::ops::brand::report_files(),
        fonts: vec![],
        output: engine::Output::Report,
    };
    match engine::page_text(&req) {
        Ok(pages) => faults(&pages),
        Err(e) => vec![format!("{kind}: the design does not lay out: {e}")],
    }
}

/// Panics with every fault found, naming the fixture
#[cfg(test)]
pub fn assert_clean(kind: &str, name: &str, data: &Value) {
    let f = layout(kind, data);
    assert!(
        f.is_empty(),
        "{name}: the report check found\n  {}",
        f.join("\n  ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(runs: &[(&str, f64, f64, [f64; 4])]) -> PageText {
        PageText {
            w: 612.0,
            h: 792.0,
            runs: runs
                .iter()
                .map(|(t, s, b, r)| (t.to_string(), *s, *b, *r))
                .collect(),
        }
    }

    #[test]
    fn it_finds_collisions_orphans_and_text_off_the_paper() {
        // Two pieces of one line abut: one piece, no collision
        let one = page(&[
            ("First ", 9.5, 100.0, [60.0, 92.0, 90.0, 102.0]),
            ("crack", 9.5, 100.0, [90.0, 92.0, 115.0, 102.0]),
            ("FC A", 6.5, 70.0, [200.0, 64.0, 216.0, 71.0]),
            // On top of FC A: a collision
            ("FC B", 6.5, 70.0, [208.0, 64.0, 224.0, 71.0]),
            ("Gone", 9.5, 300.0, [600.0, 292.0, 640.0, 302.0]),
        ]);
        let f = faults(std::slice::from_ref(&one));
        assert_eq!(f.len(), 2, "{f:?}");
        assert!(f[0].contains("\"Gone\" runs past the paper"), "{f:?}");
        assert!(f[1].contains("\"FC A\" and \"FC B\" collide"), "{f:?}");
        // A second page with one line of a note and the footer
        let note = page(&[
            ("The closing note.", 7.5, 80.0, [51.0, 74.0, 160.0, 82.0]),
            ("Page 2 of 2", 7.5, 770.0, [500.0, 763.0, 560.0, 771.0]),
        ]);
        let f = faults(&[one, note]);
        assert!(f[0].starts_with("page 2 of 2 holds only"), "{f:?}");
        assert!(f[0].contains("closing note"), "{f:?}");
    }

    /// Through Typst: two event names set on top of each other on a chart, and a closing
    /// note pushed onto a page of its own
    #[test]
    fn it_reads_real_pages() {
        let data = crate::reports::roast::build(
            &crate::reports::roast::tests::fixture(),
            &crate::reports::roast::tests::options(
                crate::reports::Units::C,
                crate::reports::Paper::Letter,
            ),
        );
        let design = r#"#import "report.typ": *
#let report(r) = page-frame(r, {
  box(width: 60mm, height: 10mm, {
    place(top + left, dx: 20mm, text(size: 7pt)[FC A])
    place(top + left, dx: 21mm, text(size: 7pt)[FC B])
  })
  v(1fr)
  pagebreak()
  text(size: 7.5pt)[The closing note.]
})"#;
        let f = layout_design(design, &data);
        assert!(
            f.iter().any(|f| f.starts_with("page 2 of 2 holds only")),
            "{f:?}"
        );
        assert!(
            f.iter()
                .any(|f| f.contains("\"FC A\" and \"FC B\" collide")),
            "{f:?}"
        );
        // The same on one page, apart: clean
        let clean = r#"#import "report.typ": *
#let report(r) = page-frame(r, {
  text(size: 7pt)[FC A] + h(5mm) + text(size: 7pt)[FC B]
})"#;
        assert_eq!(layout_design(clean, &data), Vec::<String>::new());
    }

    /// Every kind's own file runs the check on its fixtures
    #[test]
    fn every_kind_is_checked() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/reports");
        for k in super::super::KINDS {
            let src = std::fs::read_to_string(dir.join(format!("{}.rs", k.id)))
                .unwrap_or_else(|_| panic!("reports/{}.rs", k.id));
            assert!(
                src.contains("check::assert_clean(\""),
                "reports/{}.rs never runs the report check (check::assert_clean)",
                k.id
            );
        }
    }
}

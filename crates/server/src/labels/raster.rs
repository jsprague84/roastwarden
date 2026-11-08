//! Raster for printers that do not take PDF (docs/labels.md, "Printers"): PWG Raster (PWG
//! 5102.4, what IPP Everywhere printers must take) and Apple URF (what AirPrint printers
//! take; the M252dw's raster is URF and PCLm, not PWG). Both code each line the same way: a
//! byte for how many times the line repeats, then runs of a pixel repeated or of literal
//! pixels.

/// How the pixels are: 8-bit grey (one byte a pixel) or 8-bit sRGB (three)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Colour {
    Grey,
    Rgb,
}

impl Colour {
    pub fn bytes(self) -> usize {
        match self {
            Colour::Grey => 1,
            Colour::Rgb => 3,
        }
    }
}

/// One page's pixels, top row first, `width × height × colour.bytes()` long
pub struct Page<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

/// What each page says about itself
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Setup {
    pub colour: Colour,
    /// Dots per inch the pixels are at
    pub dpi: u32,
    /// Each pixel sent this many times across and down (2: 300 dpi pixels to a 600 dpi
    /// printer, a quarter of the memory)
    pub scale: u32,
    /// The page's size in points (1/72 in)
    pub page_pt: (u32, u32),
    /// PWG's `PageSizeName`, e.g. `na_letter_8.5x11in`
    pub size_name: String,
    /// PWG `MediaType` keyword (`labels`), or empty
    pub media_type: String,
    /// IPP print-quality: 3 draft, 4 normal, 5 high
    pub quality: u8,
}

/// Code one line (already scaled across) as runs: `0..=127` = that plus one of the next
/// pixel; `129..=255` = 257 minus that literal pixels follow.
fn code_line(line: &[u8], bpp: usize, out: &mut Vec<u8>) {
    let n = line.len() / bpp;
    let px = |i: usize| &line[i * bpp..(i + 1) * bpp];
    let mut i = 0;
    while i < n {
        // A run of the same pixel?
        let mut run = 1;
        while i + run < n && run < 128 && px(i + run) == px(i) {
            run += 1;
        }
        if run > 1 {
            out.push((run - 1) as u8);
            out.extend_from_slice(px(i));
            i += run;
            continue;
        }
        // Literal pixels until two in a row match (or 128 of them)
        let start = i;
        let mut count = 0;
        while i < n && count < 128 {
            if i + 1 < n && px(i) == px(i + 1) {
                break;
            }
            i += 1;
            count += 1;
        }
        if count == 0 {
            // The pair starts a run next time round
            continue;
        }
        out.push((257 - count) as u8);
        out.extend_from_slice(&line[start * bpp..(start + count) * bpp]);
    }
}

/// A page's bitmap, coded: each run of equal lines (up to 256) once, after its repeat byte
fn code_page(p: &Page, s: &Setup, out: &mut Vec<u8>) {
    let bpp = s.colour.bytes();
    let row = p.width as usize * bpp;
    let scale = s.scale.max(1) as usize;
    let mut wide = Vec::with_capacity(row * scale);
    let line_at = |y: usize| &p.pixels[y * row..(y + 1) * row];
    let mut y = 0;
    let h = p.height as usize;
    while y < h {
        // Equal source lines next to each other, each sent `scale` times
        let mut same = 1;
        while y + same < h && line_at(y + same) == line_at(y) {
            same += 1;
        }
        wide.clear();
        for x in 0..p.width as usize {
            for _ in 0..scale {
                wide.extend_from_slice(&line_at(y)[x * bpp..(x + 1) * bpp]);
            }
        }
        let mut lines = same * scale;
        while lines > 0 {
            let now = lines.min(256);
            out.push((now - 1) as u8);
            code_line(&wide, bpp, out);
            lines -= now;
        }
        y += same;
    }
}

fn put_str(h: &mut [u8], at: usize, s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(63);
    h[at..at + n].copy_from_slice(&b[..n]);
}

fn put_u32(h: &mut [u8], at: usize, v: u32) {
    h[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

/// Which raster
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Format {
    /// PWG Raster (`image/pwg-raster`)
    Pwg,
    /// Apple URF (`image/urf`)
    Urf,
}

impl Format {
    pub fn mime(self) -> &'static str {
        match self {
            Format::Pwg => "image/pwg-raster",
            Format::Urf => "image/urf",
        }
    }
}

/// The document's start: PWG's sync word, or URF's file header with the page count
pub fn begin(format: Format, pages: u32) -> Vec<u8> {
    match format {
        Format::Pwg => b"RaS2".to_vec(),
        Format::Urf => {
            let mut out = b"UNIRAST\0".to_vec();
            out.extend_from_slice(&pages.to_be_bytes());
            out
        }
    }
}

/// One page: its header and its coded bitmap, after what is in `out` (one page at a time,
/// so a long job never holds every page's pixels)
pub fn add_page(format: Format, out: &mut Vec<u8>, p: &Page, s: &Setup, pages: u32) {
    match format {
        Format::Pwg => pwg_page(out, p, s, pages),
        Format::Urf => urf_page(out, p, s),
    }
}

/// PWG Raster: `RaS2`, then each page's 1796-byte header and its coded bitmap
#[cfg(test)]
pub fn pwg(pages: &[Page], s: &Setup) -> Vec<u8> {
    let mut out = begin(Format::Pwg, pages.len() as u32);
    for p in pages {
        pwg_page(&mut out, p, s, pages.len() as u32);
    }
    out
}

fn pwg_page(out: &mut Vec<u8>, p: &Page, s: &Setup, pages: u32) {
    let scale = s.scale.max(1);
    let dpi = s.dpi * scale;
    let bpp = s.colour.bytes() as u32;
    {
        let (w, h) = (p.width * scale, p.height * scale);
        let mut hd = vec![0u8; 1796];
        put_str(&mut hd, 0, "PwgRaster");
        put_str(&mut hd, 128, &s.media_type);
        put_u32(&mut hd, 276, dpi);
        put_u32(&mut hd, 280, dpi);
        put_u32(&mut hd, 340, 1); // NumCopies
        put_u32(&mut hd, 352, s.page_pt.0);
        put_u32(&mut hd, 356, s.page_pt.1);
        put_u32(&mut hd, 372, w);
        put_u32(&mut hd, 376, h);
        put_u32(&mut hd, 384, 8); // BitsPerColor
        put_u32(&mut hd, 388, 8 * bpp); // BitsPerPixel
        put_u32(&mut hd, 392, w * bpp); // BytesPerLine
        put_u32(&mut hd, 396, 0); // ColorOrder: chunky
        put_u32(
            &mut hd,
            400,
            match s.colour {
                Colour::Grey => 18, // sGray
                Colour::Rgb => 19,  // sRGB
            },
        );
        put_u32(&mut hd, 420, bpp); // NumColors
        put_u32(&mut hd, 452, pages); // TotalPageCount
        put_u32(&mut hd, 456, 1); // CrossFeedTransform
        put_u32(&mut hd, 460, 1); // FeedTransform
        put_u32(&mut hd, 484, u32::from(s.quality)); // PrintQuality
        put_str(&mut hd, 1732, &s.size_name);
        out.extend_from_slice(&hd);
        code_page(p, s, out);
    }
}

/// Apple URF: `UNIRAST\0`, the page count, then each page's 32-byte header and its coded
/// bitmap
#[cfg(test)]
pub fn urf(pages: &[Page], s: &Setup) -> Vec<u8> {
    let mut out = begin(Format::Urf, pages.len() as u32);
    for p in pages {
        urf_page(&mut out, p, s);
    }
    out
}

fn urf_page(out: &mut Vec<u8>, p: &Page, s: &Setup) {
    let scale = s.scale.max(1);
    {
        let mut hd = [0u8; 32];
        hd[0] = (8 * s.colour.bytes()) as u8; // bits per pixel
        hd[1] = match s.colour {
            Colour::Grey => 0, // sGray
            Colour::Rgb => 1,  // sRGB
        };
        hd[2] = 1; // duplex: off
        hd[3] = s.quality;
        put_u32(&mut hd, 12, p.width * scale);
        put_u32(&mut hd, 16, p.height * scale);
        put_u32(&mut hd, 20, s.dpi * scale);
        out.extend_from_slice(&hd);
        code_page(p, s, out);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Read a coded bitmap back: `height` lines of `width` pixels of `bpp` bytes
    pub(crate) fn decode(
        mut b: &[u8],
        width: usize,
        height: usize,
        bpp: usize,
    ) -> (Vec<u8>, usize) {
        let start = b.len();
        let mut out = Vec::with_capacity(width * height * bpp);
        let mut lines = 0;
        while lines < height {
            let repeat = b[0] as usize + 1;
            b = &b[1..];
            let mut line = Vec::with_capacity(width * bpp);
            while line.len() < width * bpp {
                let c = b[0];
                b = &b[1..];
                match c.cmp(&128) {
                    std::cmp::Ordering::Less => {
                        let px = &b[..bpp];
                        for _ in 0..=c {
                            line.extend_from_slice(px);
                        }
                        b = &b[bpp..];
                    }
                    std::cmp::Ordering::Greater => {
                        let n = 257 - c as usize;
                        line.extend_from_slice(&b[..n * bpp]);
                        b = &b[n * bpp..];
                    }
                    std::cmp::Ordering::Equal => line.resize(width * bpp, 0xff),
                }
            }
            for _ in 0..repeat {
                out.extend_from_slice(&line);
            }
            lines += repeat;
        }
        (out, start - b.len())
    }

    fn setup(colour: Colour, scale: u32) -> Setup {
        Setup {
            colour,
            dpi: 300,
            scale,
            page_pt: (612, 792),
            size_name: "na_letter_8.5x11in".into(),
            media_type: "labels".into(),
            quality: 4,
        }
    }

    #[test]
    fn runs_and_literals_code_as_the_spec_says() {
        // 8 grey pixels: three white, then 1 2 3, then two black
        let mut out = vec![];
        code_line(&[255, 255, 255, 1, 2, 3, 0, 0], 1, &mut out);
        assert_eq!(out, vec![2, 255, 254, 1, 2, 3, 1, 0]);
        // A line of one colour is one run (128 at most each)
        let mut out = vec![];
        code_line(&[7; 300], 1, &mut out);
        assert_eq!(out, vec![127, 7, 127, 7, 43, 7]);
    }

    #[test]
    fn a_page_reads_back_as_it_was_scaled_or_not() {
        let (w, h) = (37u32, 23u32);
        let mut px = vec![255u8; (w * h * 3) as usize];
        // A few marks: a black square, a red line, noise
        for y in 5..12 {
            for x in 3..9 {
                let i = ((y * w + x) * 3) as usize;
                px[i..i + 3].copy_from_slice(&[0, 0, 0]);
            }
        }
        for x in 0..w {
            let i = ((17 * w + x) * 3) as usize;
            px[i..i + 3].copy_from_slice(&[200, 30, 30]);
        }
        for (k, v) in px.iter_mut().enumerate().skip(400).step_by(97) {
            *v = (k % 251) as u8;
        }
        let page = Page {
            width: w,
            height: h,
            pixels: &px,
        };
        let doc = pwg(&[page], &setup(Colour::Rgb, 1));
        assert_eq!(&doc[..4], b"RaS2");
        let hd = &doc[4..4 + 1796];
        assert_eq!(&hd[..9], b"PwgRaster");
        assert_eq!(u32::from_be_bytes(hd[372..376].try_into().unwrap()), w);
        assert_eq!(u32::from_be_bytes(hd[400..404].try_into().unwrap()), 19);
        let (back, _) = decode(&doc[4 + 1796..], w as usize, h as usize, 3);
        assert_eq!(back, px);

        // Doubled for a 600 dpi printer: each pixel twice across and each line twice
        let page = Page {
            width: w,
            height: h,
            pixels: &px,
        };
        let doc = urf(&[page], &setup(Colour::Rgb, 2));
        assert_eq!(&doc[..8], b"UNIRAST\0");
        assert_eq!(u32::from_be_bytes(doc[8..12].try_into().unwrap()), 1);
        let hd = &doc[12..44];
        assert_eq!(hd[0], 24);
        assert_eq!(u32::from_be_bytes(hd[12..16].try_into().unwrap()), 2 * w);
        assert_eq!(u32::from_be_bytes(hd[20..24].try_into().unwrap()), 600);
        let (back, _) = decode(&doc[44..], 2 * w as usize, 2 * h as usize, 3);
        for y in 0..(2 * h) as usize {
            for x in 0..(2 * w) as usize {
                let a = &back[(y * 2 * w as usize + x) * 3..][..3];
                let b = &px[((y / 2) * w as usize + x / 2) * 3..][..3];
                assert_eq!(a, b, "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn a_white_page_is_small() {
        let (w, h) = (2550u32, 3300u32);
        let px = vec![255u8; (w * h) as usize];
        let doc = pwg(
            &[Page {
                width: w,
                height: h,
                pixels: &px,
            }],
            &setup(Colour::Grey, 1),
        );
        // 3300 lines in runs of 256: 13 lines of 20 run bytes each, plus the header
        assert!(doc.len() < 1796 + 4 + 13 * 50, "{}", doc.len());
    }
}

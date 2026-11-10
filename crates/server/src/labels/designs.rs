//! The designs: the built-in Typst files and the owner's own in the data folder, with their
//! fonts; a design's id and name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_bytes::ByteBuf;
use utoipa::ToSchema;

use super::{BUILTIN_DESIGNS, engine};

/// A design to choose from.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Design {
    pub id: String,
    pub name: String,
    /// Shipped with Roastwarden (else the owner's, from the labels folder)
    pub builtin: bool,
}

/// The labels folder beside the database: `designs/*.typ` and `fonts/*.ttf|otf`.
#[derive(Debug, Clone)]
pub struct Folder(pub PathBuf);

impl Folder {
    fn designs(&self) -> PathBuf {
        self.0.join("designs")
    }

    fn fonts(&self) -> PathBuf {
        self.0.join("fonts")
    }

    /// The built-in designs and the owner's (`designs/<id>.typ`; an owner's `plain.typ`
    /// replaces the built-in one).
    pub fn list(&self) -> Vec<Design> {
        let mut out: Vec<Design> = BUILTIN_DESIGNS
            .iter()
            .map(|(id, name, _)| Design {
                id: id.to_string(),
                name: name.to_string(),
                builtin: true,
            })
            .collect();
        let mut own: Vec<String> = std::fs::read_dir(self.designs())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                (p.extension()? == "typ")
                    .then(|| p.file_stem()?.to_str().map(str::to_string))
                    .flatten()
            })
            .filter(|id| valid_id(id))
            .collect();
        own.sort();
        for id in own {
            out.retain(|d| d.id != id);
            out.push(Design {
                name: id.clone(),
                id,
                builtin: false,
            });
        }
        out
    }

    /// A design's source and the images of the folder it names (none for a built-in).
    pub fn design(&self, id: &str) -> Option<DesignSource> {
        if !valid_id(id) {
            return None;
        }
        let path = self.designs().join(format!("{id}.typ"));
        if let Ok(source) = std::fs::read_to_string(&path) {
            let files = named_files(&source, &self.designs());
            return Some(DesignSource {
                id: id.to_string(),
                version: None,
                source,
                files,
            });
        }
        builtin_design(id)
    }

    /// The owner's font files, read at each render so a new one needs no restart.
    pub fn own_fonts(&self) -> Vec<Vec<u8>> {
        std::fs::read_dir(self.fonts())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| {
                        matches!(
                            x.to_ascii_lowercase().as_str(),
                            "ttf" | "otf" | "ttc" | "otc"
                        )
                    })
            })
            .filter_map(|e| std::fs::read(e.path()).ok())
            .collect()
    }
}

/// A design ready to render: its source and the files it reads by name.
#[derive(Debug, Clone)]
pub struct DesignSource {
    pub id: String,
    /// Its saved version (none for a built-in, a folder design or a one-off)
    pub version: Option<i64>,
    pub source: String,
    pub files: BTreeMap<String, ByteBuf>,
}

impl DesignSource {
    /// `id`, or `id@version` for a saved one
    pub fn label(&self) -> String {
        match self.version {
            Some(v) => format!("{}@{v}", self.id),
            None => self.id.clone(),
        }
    }
}

pub fn builtin_design(id: &str) -> Option<DesignSource> {
    BUILTIN_DESIGNS
        .iter()
        .find(|(d, _, _)| *d == id)
        .map(|(id, _, src)| DesignSource {
            id: id.to_string(),
            version: None,
            source: src.to_string(),
            files: BTreeMap::new(),
        })
}

/// The files of `dir` whose names the source mentions (`image("logo.svg")`): all a design
/// can read, without sending the whole folder to every render.
fn named_files(source: &str, dir: &Path) -> BTreeMap<String, ByteBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let wanted = !name.ends_with(".typ") && source.contains(&name);
            let bytes = wanted.then(|| std::fs::read(e.path()).ok()).flatten()?;
            Some((name, ByteBuf::from(bytes)))
        })
        .collect()
}

/// An id from a name: "HP LaserJet (office)" → "hp-laserjet-office"
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out
        .trim_end_matches('-')
        .chars()
        .take(64)
        .collect::<String>();
    if out.is_empty() {
        "printer".into()
    } else {
        out
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Every font family a design can name: the built-in ones and these.
pub fn font_families(extra: &[Vec<u8>]) -> Vec<String> {
    let extra: Vec<typst::text::Font> = extra
        .iter()
        .flat_map(|b| typst::text::Font::iter(typst::foundations::Bytes::new(b.clone())))
        .collect();
    let mut fams: Vec<String> = engine::builtin_fonts()
        .iter()
        .chain(extra.iter())
        .map(|f| f.info().family.clone())
        .collect();
    fams.sort();
    fams.dedup();
    fams
}

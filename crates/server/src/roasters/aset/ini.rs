//! A Qt `QSettings` INI file (an `.aset`): sections of keys with their raw values, read
//! as text, integers, flags and Qt's escaped lists.

use std::collections::HashMap;

use super::qt::qt_values;

/// A Qt `QSettings` INI file: sections of keys with their raw (still escaped) values.
#[derive(Debug, Default)]
pub struct Ini {
    sections: HashMap<String, HashMap<String, String>>,
}

impl Ini {
    pub fn parse(text: &str) -> Ini {
        let mut ini = Ini::default();
        let mut section = String::new();
        for line in text.lines() {
            let s = line.trim();
            if s.is_empty() || s.starts_with(';') || s.starts_with('#') {
                continue;
            }
            if s.starts_with('[') && s.ends_with(']') {
                section = s[1..s.len() - 1].trim().to_string();
                continue;
            }
            if let Some((k, v)) = s.split_once('=') {
                ini.sections
                    .entry(section.clone())
                    .or_default()
                    .insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        ini
    }

    fn raw(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(section)?.get(key).map(String::as_str)
    }

    /// A value as a list (one element for a plain value, none when absent).
    pub fn list(&self, section: &str, key: &str) -> Vec<String> {
        self.raw(section, key).map(qt_values).unwrap_or_default()
    }

    /// A value as one string (a list's elements joined back with `, `).
    pub fn text(&self, section: &str, key: &str) -> Option<String> {
        let l = self.list(section, key);
        (!l.is_empty()).then(|| l.join(", "))
    }

    pub fn num(&self, section: &str, key: &str) -> Option<f64> {
        self.text(section, key)?.trim().parse().ok()
    }

    pub fn int(&self, section: &str, key: &str) -> Option<i64> {
        let n = self.num(section, key)?;
        (n.fract() == 0.0).then_some(n as i64)
    }

    pub fn flag(&self, section: &str, key: &str) -> bool {
        self.text(section, key)
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("true") || v.trim() == "1")
    }
}

//! PATCH bodies' fields that can be left out, cleared with `null` or set.

use serde::Deserialize;

/// `Option<Option<T>>` for PATCH-like fields: absent → `None`, `null` →
/// `Some(None)`, value → `Some(Some(v))`. Use with `#[serde(default)]`.
pub fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

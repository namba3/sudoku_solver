use std::path::Path;

pub(super) fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    crate::persistence::write_atomic(path, bytes).map_err(|error| error.to_string())
}

pub(super) fn write_json(
    path: &Path,
    value: &serde_json::Value,
    trailing_newline: bool,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if trailing_newline {
        bytes.push(b'\n');
    }
    write_bytes(path, &bytes)
}

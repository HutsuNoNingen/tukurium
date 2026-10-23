//! Validation shared by peer transfers and the asset service.
use serde_json::Value;
pub const MAX_ASSET: usize = 128 * 1024 * 1024;

pub fn validate_glb(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 20
        || bytes.len() > MAX_ASSET
        || &bytes[..4] != b"glTF"
        || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != 2
        || u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize != bytes.len()
    {
        return Err("Choose a self-contained glTF 2.0 binary (.glb), up to 128 MiB".into());
    }
    let mut offset = 12;
    let mut document = None;
    while offset < bytes.len() {
        if offset + 8 > bytes.len() {
            return Err("Truncated GLB chunk".into());
        }
        let length = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        let end = (offset + 8)
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or("Invalid GLB chunk length")?;
        if offset == 12 && kind != b"JSON" {
            return Err("GLB must start with JSON".into());
        }
        if kind == b"JSON" {
            if document.is_some() {
                return Err("Duplicate GLB document".into());
            }
            document = Some(
                serde_json::from_slice::<Value>(&bytes[offset + 8..end])
                    .map_err(|_| "Invalid GLB JSON")?,
            );
        }
        offset = end;
    }
    let document = document.ok_or("Missing GLB document")?;
    if document["asset"]["version"] != "2.0"
        || !document["meshes"].as_array().is_some_and(|m| !m.is_empty())
    {
        return Err("GLB must contain a glTF 2.0 mesh".into());
    }
    for collection in ["buffers", "images"] {
        if let Some(items) = document[collection].as_array() {
            for item in items {
                if let Some(uri) = item.get("uri")
                    && (collection == "buffers"
                        || !uri.as_str().is_some_and(|s| {
                            s.starts_with("data:image/png;base64,")
                                || s.starts_with("data:image/jpeg;base64,")
                        }))
                {
                    return Err(
                        "GLB dependencies must be embedded; external files are unsupported".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

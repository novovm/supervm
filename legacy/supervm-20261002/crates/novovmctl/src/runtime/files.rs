use serde::Serialize;

use crate::error::CtlError;

#[allow(dead_code)]
pub fn write_json_pretty<T: Serialize>(path: &str, value: &T) -> Result<(), CtlError> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|e| CtlError::FileWriteFailed(format!("serialize json for `{path}`: {e}")))?;

    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CtlError::FileWriteFailed(format!("create parent dir for `{path}`: {e}"))
        })?;
    }

    std::fs::write(path, rendered)
        .map_err(|e| CtlError::FileWriteFailed(format!("write `{path}`: {e}")))?;

    Ok(())
}

use serde::Serialize;
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;

use crate::error::CtlError;
use crate::output::{host_name, now_unix_ms};

pub fn append_success_jsonl<T: Serialize>(
    path: &str,
    command: &str,
    data: &T,
) -> Result<(), CtlError> {
    let envelope = json!({
        "ok": true,
        "command": command,
        "timestamp_unix_ms": now_unix_ms(),
        "host": host_name(),
        "data": data,
    });

    append_jsonl_value(path, &envelope)
}

pub fn append_error_jsonl(path: &str, command: &str, err: &CtlError) -> Result<(), CtlError> {
    let envelope = json!({
        "ok": false,
        "command": command,
        "timestamp_unix_ms": now_unix_ms(),
        "host": host_name(),
        "error": {
            "kind": error_kind(err),
            "message": err.to_string()
        }
    });

    append_jsonl_value(path, &envelope)
}

pub fn append_jsonl_value(path: &str, value: &Value) -> Result<(), CtlError> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CtlError::FileWriteFailed(format!("create audit parent dir `{path}`: {e}"))
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| CtlError::FileWriteFailed(format!("open audit file `{path}`: {e}")))?;

    let line = serde_json::to_string(value)
        .map_err(|e| CtlError::FileWriteFailed(format!("serialize audit jsonl: {e}")))?;

    writeln!(file, "{line}")
        .map_err(|e| CtlError::FileWriteFailed(format!("append audit jsonl `{path}`: {e}")))?;

    Ok(())
}

fn error_kind(err: &CtlError) -> &'static str {
    match err {
        CtlError::InvalidArgument(_) => "InvalidArgument",
        CtlError::FileReadFailed(_) => "FileReadFailed",
        CtlError::FileWriteFailed(_) => "FileWriteFailed",
        CtlError::BinaryNotFound(_) => "BinaryNotFound",
        CtlError::ProcessLaunchFailed(_) => "ProcessLaunchFailed",
        CtlError::IntegrationFailed(_) => "IntegrationFailed",
    }
}

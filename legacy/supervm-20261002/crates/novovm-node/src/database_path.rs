#[cfg(windows)]
use anyhow::bail;
use anyhow::Result;
#[cfg(windows)]
use std::path::Component;
use std::path::PathBuf;

/// Windows canonicalization returns a verbatim path. RocksDB's Windows backend
/// appends slash-separated children (e.g. `/LOG`), which verbatim paths reject.
/// Convert only the prefix of an already resolved filesystem path; do not use
/// this function as a substitute for canonicalization or isolation checks.
#[cfg(windows)]
pub(crate) fn native_database_path(canonical: PathBuf) -> Result<PathBuf> {
    use std::ffi::OsString;
    use std::path::Prefix;
    let mut components = canonical.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        bail!("canonical database path has no Windows volume prefix");
    };
    let mut native = match prefix.kind() {
        Prefix::VerbatimDisk(letter) => PathBuf::from(format!("{}:\\", char::from(letter))),
        Prefix::VerbatimUNC(server, share) => {
            let mut prefix = OsString::from(r"\\");
            prefix.push(server);
            prefix.push(r"\");
            prefix.push(share);
            prefix.push(r"\");
            PathBuf::from(prefix)
        }
        Prefix::Disk(_) | Prefix::UNC(_, _) => return Ok(canonical),
        _ => bail!("database path uses an unsupported Windows device namespace"),
    };
    for component in components {
        if component != Component::RootDir {
            native.push(component.as_os_str());
        }
    }
    Ok(native)
}

#[cfg(not(windows))]
pub(crate) fn native_database_path(canonical: PathBuf) -> Result<PathBuf> {
    Ok(canonical)
}

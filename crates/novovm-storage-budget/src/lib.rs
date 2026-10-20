//! App-directory quota over the existing SQLite VFS and ordinary file writers.
//! Configure before opening any App database. This is a single-process owner;
//! other processes are excluded by the root lock. Symlink targets are excluded.
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    ffi::{c_char, c_int, c_void, CString},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub const MIN_LIMIT_BYTES: u64 = 500 * 1024 * 1024;
const POLICY_FILE: &str = ".novovm-storage-policy";
const LOCK_FILE: &str = ".novovm-storage-owner";
const SLOT_BYTES: usize = 64;
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Sqlite(i32),
    InvalidLimit,
    InvalidPath,
    CorruptPolicy,
    Busy,
    Poisoned,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "storage budget: {self:?}")
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

unsafe extern "C" {
    fn sqlite3_quota_initialize(original: *const c_char, default: c_int) -> c_int;
    fn sqlite3_quota_set(
        pattern: *const c_char,
        limit: i64,
        callback: Option<unsafe extern "C" fn(*const c_char, *mut i64, i64, *mut c_void)>,
        arg: *mut c_void,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> c_int;
    fn novovm_quota_limits(pattern: *const c_char, normal: i64, auxiliary: i64) -> c_int;
    fn novovm_quota_snapshot(pattern: *const c_char, used: *mut i64, reserved: *mut i64) -> c_int;
    fn novovm_quota_reserve(pattern: *const c_char, bytes: i64) -> c_int;
    fn novovm_quota_release(pattern: *const c_char, bytes: i64);
    fn novovm_quota_refresh(path: *const c_char) -> c_int;
    fn novovm_quota_revision() -> i64;
}
fn check(rc: i32) -> Result<()> {
    if rc == libsqlite3_sys::SQLITE_OK {
        Ok(())
    } else if rc == libsqlite3_sys::SQLITE_FULL {
        Err(Error::Io(std::io::ErrorKind::StorageFull.into()))
    } else {
        Err(Error::Sqlite(rc))
    }
}
static INIT: OnceLock<i32> = OnceLock::new();
static ROOTS: OnceLock<Mutex<HashMap<PathBuf, Arc<Inner>>>> = OnceLock::new();
fn roots() -> &'static Mutex<HashMap<PathBuf, Arc<Inner>>> {
    ROOTS.get_or_init(Default::default)
}
fn init() -> Result<()> {
    check(*INIT.get_or_init(|| unsafe {
        let rc = libsqlite3_sys::sqlite3_initialize();
        if rc != 0 {
            return rc;
        }
        sqlite3_quota_initialize(std::ptr::null(), 1)
    }))
}

#[derive(Clone)]
pub struct DiskBudget(Arc<Inner>);
struct Inner {
    root: PathBuf,
    pattern: CString,
    state: Mutex<State>,
    _lock: File,
}
struct State {
    limit: Option<u64>,
    generation: u64,
    known: HashSet<PathBuf>,
    grandfather: u64,
    pending_reconcile: Vec<(Vec<PathBuf>, u64)>,
    aliases: HashSet<PathBuf>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub limit_bytes: Option<u64>,
    pub used_bytes: u64,
    pub reserved_bytes: u64,
    pub maintenance_bytes: u64,
    pub over_limit: bool,
    pub blocked: bool,
}

fn clean(path: &Path) -> Result<PathBuf> {
    let s = path.to_str().ok_or(Error::InvalidPath)?;
    #[cfg(windows)]
    let s = s.strip_prefix(r"\\?\").unwrap_or(s);
    Ok(PathBuf::from(s))
}

fn normalized_candidate(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(Error::InvalidPath);
    }
    let mut missing = Vec::new();
    let mut existing = path;
    while !existing.exists() {
        missing.push(
            existing
                .file_name()
                .ok_or(Error::InvalidPath)?
                .to_os_string(),
        );
        existing = existing.parent().ok_or(Error::InvalidPath)?;
    }
    let mut result = clean(&fs::canonicalize(existing)?)?;
    for part in missing.into_iter().rev() {
        result.push(part);
    }
    Ok(result)
}
fn cpath(path: &Path) -> Result<CString> {
    CString::new(clean(path)?.to_str().ok_or(Error::InvalidPath)?).map_err(|_| Error::InvalidPath)
}
fn pattern(root: &Path) -> Result<CString> {
    let value = clean(root)?
        .to_str()
        .ok_or(Error::InvalidPath)?
        .replace('\\', "/");
    let mut escaped = String::new();
    for c in value.chars() {
        match c {
            '*' => escaped.push_str("[*]"),
            '?' => escaped.push_str("[?]"),
            '[' => escaped.push_str("[[]"),
            _ => escaped.push(c),
        }
    }
    escaped.push_str("/*");
    CString::new(escaped).map_err(|_| Error::InvalidPath)
}
fn validate(limit: Option<u64>) -> Result<()> {
    if limit.is_some_and(|v| v < MIN_LIMIT_BYTES || v > i64::MAX as u64) {
        Err(Error::InvalidLimit)
    } else {
        Ok(())
    }
}
fn headroom(limit: Option<u64>) -> u64 {
    limit
        .map(|n| (n / 100).clamp(1024 * 1024, 16 * 1024 * 1024))
        .unwrap_or(0)
}
fn walk(dir: &Path, files: &mut HashSet<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let meta = match fs::symlink_metadata(entry.path()) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            walk(&entry.path(), files)?;
        } else if meta.is_file() {
            files.insert(clean(&entry.path())?);
        }
    }
    Ok(())
}
fn refresh(path: &Path) -> Result<()> {
    let p = cpath(path)?;
    check(unsafe { novovm_quota_refresh(p.as_ptr()) })
}
fn counters(inner: &Inner) -> Result<(u64, u64)> {
    let (mut u, mut r) = (0, 0);
    check(unsafe { novovm_quota_snapshot(inner.pattern.as_ptr(), &mut u, &mut r) })?;
    Ok((u as u64, r as u64))
}
fn apply(inner: &Inner, state: &State) -> Result<()> {
    let cap = state.limit.unwrap_or(i64::MAX as u64);
    let reserve = headroom(state.limit);
    let aux = cap
        .max(state.grandfather.saturating_add(reserve))
        .min(i64::MAX as u64);
    check(unsafe {
        novovm_quota_limits(
            inner.pattern.as_ptr(),
            cap.saturating_sub(reserve) as i64,
            aux as i64,
        )
    })
}

impl DiskBudget {
    /// Install default quota VFS and inventory all existing files exactly once.
    /// Repeated calls for the same root reuse the owner, never reset user choice.
    pub fn configure(root: impl AsRef<Path>) -> Result<Self> {
        init()?;
        let supplied = root.as_ref();
        let alias = clean(&if supplied.is_absolute() {
            supplied.to_path_buf()
        } else {
            std::env::current_dir()?.join(supplied)
        })?;
        let root = clean(&fs::canonicalize(&alias)?)?;
        let mut registry = roots().lock().map_err(|_| Error::Poisoned)?;
        if let Some(existing) = registry.get(&root) {
            existing
                .state
                .lock()
                .map_err(|_| Error::Poisoned)?
                .aliases
                .insert(alias);
            return Ok(Self(existing.clone()));
        }
        if registry
            .keys()
            .any(|r| root.starts_with(r) || r.starts_with(&root))
        {
            return Err(Error::InvalidPath);
        }
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(root.join(LOCK_FILE))?;
        lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
        let (limit, generation) = load_policy(&root)?;
        let pat = pattern(&root)?;
        check(unsafe {
            sqlite3_quota_set(pat.as_ptr(), i64::MAX, None, std::ptr::null_mut(), None)
        })?;
        let mut known = HashSet::new();
        walk(&root, &mut known)?;
        for path in &known {
            refresh(path)?;
        }
        let inner = Arc::new(Inner {
            root: root.clone(),
            pattern: pat,
            state: Mutex::new(State {
                limit,
                generation,
                known,
                grandfather: 0,
                pending_reconcile: Vec::new(),
                aliases: HashSet::from([alias, root.clone()]),
            }),
            _lock: lock,
        });
        {
            let mut state = inner.state.lock().map_err(|_| Error::Poisoned)?;
            let used = counters(&inner)?.0;
            if limit.is_some_and(|n| used > n) {
                state.grandfather = used;
            }
            apply(&inner, &state)?;
        }
        registry.insert(root, inner.clone());
        Ok(Self(inner))
    }
    /// Explicit UI/maintenance snapshot; scans names/metadata, never file bodies.
    /// VFS writes do not scan directories. Reconciles removed SHM/cache files.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
        let mut now = HashSet::new();
        walk(&self.0.root, &mut now)?;
        state.known.extend(now.iter().cloned());
        for path in &state.known {
            refresh(path)?;
        }
        // Failed external stat never releases unaccounted growth. Retry only
        // these captured paths before making their reservation available again.
        for (paths, _) in &state.pending_reconcile {
            for path in paths {
                refresh(path)?;
            }
        }
        for (_, bytes) in state.pending_reconcile.drain(..) {
            unsafe { novovm_quota_release(self.0.pattern.as_ptr(), bytes as i64) };
        }
        state.known = now;
        let (used, reserved) = counters(&self.0)?;
        // Grandfather allowance only contracts as retained data is freed.
        if state.grandfather > used {
            state.grandfather = used;
            apply(&self.0, &state)?;
        }
        let maintenance = headroom(state.limit);
        Ok(Snapshot {
            limit_bytes: state.limit,
            used_bytes: used,
            reserved_bytes: reserved,
            maintenance_bytes: maintenance,
            over_limit: state.limit.is_some_and(|n| used > n),
            blocked: state
                .limit
                .is_some_and(|n| used.saturating_add(reserved) >= n.saturating_sub(maintenance)),
        })
    }
    /// Two checksummed fixed-size slots preserve the previous policy on a torn
    /// write. Changing the limit never needs free space or deletes stored data.
    pub fn set_limit(&self, limit: Option<u64>) -> Result<Snapshot> {
        validate(limit)?;
        {
            let mut state = self.0.state.lock().map_err(|_| Error::Poisoned)?;
            let generation = state
                .generation
                .checked_add(1)
                .ok_or(Error::CorruptPolicy)?;
            store_policy(&self.0.root, limit, generation)?;
            state.limit = limit;
            state.generation = generation;
            let used = counters(&self.0)?.0;
            state.grandfather = if limit.is_some_and(|n| used > n) {
                used
            } else {
                0
            };
            apply(&self.0, &state)?;
        }
        self.snapshot()
    }
}

fn slot(limit: Option<u64>, generation: u64) -> [u8; SLOT_BYTES] {
    let mut b = [0; SLOT_BYTES];
    b[..8].copy_from_slice(&generation.to_le_bytes());
    b[8..16].copy_from_slice(&limit.unwrap_or(u64::MAX).to_le_bytes());
    b[16..24].copy_from_slice(b"NOVOQU01");
    let hash = Sha256::digest(&b[..32]);
    b[32..].copy_from_slice(&hash);
    b
}
fn decode(b: &[u8]) -> Option<(Option<u64>, u64)> {
    if b.len() != SLOT_BYTES
        || &b[16..24] != b"NOVOQU01"
        || Sha256::digest(&b[..32]).as_slice() != &b[32..]
    {
        return None;
    }
    let n = u64::from_le_bytes(b[8..16].try_into().ok()?);
    let limit = if n == u64::MAX { None } else { Some(n) };
    validate(limit).ok()?;
    Some((limit, u64::from_le_bytes(b[..8].try_into().ok()?)))
}
fn load_policy(root: &Path) -> Result<(Option<u64>, u64)> {
    let path = root.join(POLICY_FILE);
    if !path.exists() {
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        f.write_all(&slot(None, 0))?;
        f.write_all(&slot(None, 0))?;
        f.sync_all()?;
        #[cfg(unix)]
        File::open(root)?.sync_all()?;
        return Ok((None, 0));
    }
    let mut b = [0; SLOT_BYTES * 2];
    File::open(path)?.read_exact(&mut b)?;
    [decode(&b[..64]), decode(&b[64..])]
        .into_iter()
        .flatten()
        .max_by_key(|x| x.1)
        .ok_or(Error::CorruptPolicy)
}
fn store_policy(root: &Path, limit: Option<u64>, generation: u64) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .open(root.join(POLICY_FILE))?;
    f.seek(SeekFrom::Start((generation % 2) * SLOT_BYTES as u64))?;
    f.write_all(&slot(limit, generation))?;
    f.sync_all()?;
    Ok(())
}

pub struct Reservation {
    entries: Vec<(Arc<Inner>, Vec<PathBuf>, u64)>,
}
impl Reservation {
    pub fn finish(mut self) -> Result<()> {
        self.release()
    }
    fn release(&mut self) -> Result<()> {
        let mut error = None;
        for (inner, paths, bytes) in self.entries.drain(..) {
            let mut reconciled = true;
            if let Ok(mut state) = inner.state.lock() {
                for p in &paths {
                    if let Err(e) = refresh(p) {
                        error = Some(e);
                        reconciled = false;
                    }
                    state.known.insert(p.clone());
                }
                if !reconciled {
                    state.pending_reconcile.push((paths, bytes));
                }
            } else {
                error = Some(Error::Poisoned);
                reconciled = false;
            }
            if reconciled {
                unsafe { novovm_quota_release(inner.pattern.as_ptr(), bytes as i64) };
            }
        }
        error.map_or(Ok(()), Err)
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// Reserve peak additional physical bytes, including AtomicFile .new/.bak.
/// Writers must not exceed their reservation and must list all mutated paths.
/// Outside registered roots this is a no-op. No filesystem body is scanned.
pub fn reserve_for_paths(paths: &[PathBuf], extra_bytes: u64) -> Result<Reservation> {
    if extra_bytes > i64::MAX as u64 {
        return Err(Error::InvalidLimit);
    }
    let registry = roots().lock().map_err(|_| Error::Poisoned)?;
    let mut reservation = Reservation {
        entries: Vec::new(),
    };
    for inner in registry.values() {
        let mut matches = Vec::new();
        for path in paths {
            let lexical = clean(path)?;
            let path = normalized_candidate(path)?;
            let lexical_inside = inner
                .state
                .lock()
                .map_err(|_| Error::Poisoned)?
                .aliases
                .iter()
                .any(|alias| lexical.starts_with(alias));
            if lexical_inside && !path.starts_with(&inner.root) {
                return Err(Error::InvalidPath);
            }
            if path.starts_with(&inner.root) {
                if path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(Error::InvalidPath);
                }
                let mut ancestor = path.as_path();
                while ancestor != inner.root {
                    if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
                        return Err(Error::InvalidPath);
                    }
                    ancestor = ancestor.parent().ok_or(Error::InvalidPath)?;
                }
                matches.push(path);
            }
        }
        if !matches.is_empty() {
            check(unsafe { novovm_quota_reserve(inner.pattern.as_ptr(), extra_bytes as i64) })?;
            reservation
                .entries
                .push((inner.clone(), matches, extra_bytes));
        }
    }
    Ok(reservation)
}
/// Only persistent capacity changes/limit updates wake blocked work. Temporary
/// rollback journal churn is deliberately excluded to avoid retry loops.
pub fn capacity_revision() -> u64 {
    if INIT.get().is_none() {
        0
    } else {
        unsafe { novovm_quota_revision() as u64 }
    }
}

#[cfg(test)]
mod tests;

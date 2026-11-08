//! Bounded, no-follow, owner-checked ledger files.
//!
//! This is the in-process form of `ledger_io.py`. Directories are opened from
//! `/` with `O_NOFOLLOW`. A live save copies the current bytes onto the backup
//! and only then replaces the ledger. An empty seed is not written over a
//! missing or damaged file.

use std::io::ErrorKind;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::ledger::{self, Ledger, load_strict_bytes};

pub const LEDGER_MAX: usize = 8 * 1024 * 1024;
pub const QFX_MAX: usize = 16 * 1024 * 1024;
const MAX_ALLOWED: usize = 64 * 1024 * 1024;

pub const CODE_ERR: i32 = 1;
pub const CODE_MISSING: i32 = 2;
pub const CODE_LIMIT: i32 = 3;
pub const CODE_TYPE: i32 = 4;
pub const CODE_PERM: i32 = 5;
pub const CODE_USAGE: i32 = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IoError {
    pub code: i32,
    pub message: String,
}

impl IoError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub ledger: PathBuf,
    pub backup: PathBuf,
}

impl Paths {
    pub fn from_home(home: &str) -> Option<Self> {
        let home = ledger::normalize_path(home);
        if !home.starts_with('/') || home == "/" {
            return None;
        }
        let dir = PathBuf::from(home).join(".local/share/expenses");
        Some(Self {
            ledger: dir.join("ledger.json"),
            backup: dir.join("ledger.json.bak"),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Storage {
    Disk(Paths),
    Memory,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileSig {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub state: Ledger,
    pub status: String,
    pub block_empty_persist: bool,
    pub storage: Storage,
    signature: Option<FileSig>,
}

impl Session {
    pub fn from_env() -> Self {
        match std::env::var("HOME") {
            Ok(home) => match Paths::from_home(&home) {
                Some(paths) => Self::open(Storage::Disk(paths)),
                None => Self::unavailable(),
            },
            Err(_) => Self::unavailable(),
        }
    }

    pub fn open(storage: Storage) -> Self {
        let mut session = Self {
            state: Ledger::empty(),
            status: String::new(),
            block_empty_persist: false,
            storage,
            signature: None,
        };
        if matches!(session.storage, Storage::Disk(_)) {
            session.reload();
        }
        session
    }

    pub fn unavailable() -> Self {
        Self {
            state: Ledger::empty(),
            status: "HOME is unset. Showing empty ledger.".into(),
            block_empty_persist: true,
            storage: Storage::Unavailable,
            signature: None,
        }
    }

    pub fn reload(&mut self) {
        let Storage::Disk(paths) = self.storage.clone() else {
            return;
        };
        let mut wrote = false;
        match read_file(&paths.ledger, LEDGER_MAX) {
            Ok(bytes) => match load_strict_bytes(&bytes) {
                outcome if outcome.ok => self.accept(outcome.state, ""),
                outcome => self.try_backup(&paths, &format!("{}.", outcome.error), &mut wrote),
            },
            Err(error) if error.code == CODE_MISSING => {
                self.try_backup(&paths, "No ledger file.", &mut wrote)
            }
            Err(error) => self.try_backup(&paths, &format!("{}.", error.message), &mut wrote),
        }
        if !wrote {
            self.signature = file_sig(&paths.ledger);
        }
    }

    pub fn reload_if_changed(&mut self) -> bool {
        let Storage::Disk(paths) = &self.storage else {
            return false;
        };
        if file_sig(&paths.ledger) == self.signature {
            return false;
        }
        self.reload();
        true
    }

    pub fn apply(&mut self, next: Ledger, message: &str) -> bool {
        self.state = next;
        if !self.persist() {
            return false;
        }
        if !message.is_empty() {
            self.status = message.to_string();
        }
        true
    }

    pub fn persist(&mut self) -> bool {
        match &self.storage {
            Storage::Unavailable => {
                self.status = "HOME is unset; cannot save ledger.".into();
                false
            }
            Storage::Memory => {
                self.block_empty_persist = false;
                true
            }
            Storage::Disk(paths) => {
                if self.block_empty_persist && self.state.is_effectively_empty() {
                    self.status =
                        "Not saving empty ledger over a missing/damaged file. Add an entry first."
                            .into();
                    return false;
                }
                let paths = paths.clone();
                let payload = ledger::dump(&self.state);
                self.block_empty_persist = false;
                match save_file(&paths.ledger, &paths.backup, payload.as_bytes(), LEDGER_MAX) {
                    Ok(()) => {
                        self.signature = file_sig(&paths.ledger);
                        true
                    }
                    Err(error) => {
                        self.status = format!("Could not save ledger: {}", error.message);
                        false
                    }
                }
            }
        }
    }

    fn accept(&mut self, state: Ledger, message: &str) {
        self.state = state;
        self.block_empty_persist = false;
        if !message.is_empty() {
            self.status = message.to_string();
        }
    }

    fn seed_blocked(&mut self, message: &str) {
        self.state = Ledger::empty();
        self.block_empty_persist = true;
        self.status = message.to_string();
    }

    fn try_backup(&mut self, paths: &Paths, reason: &str, wrote: &mut bool) {
        self.status = format!("{reason} Trying backup…");
        match read_file(&paths.backup, LEDGER_MAX) {
            Ok(bytes) => match load_strict_bytes(&bytes) {
                outcome if outcome.ok && !outcome.state.is_effectively_empty() => {
                    self.accept(outcome.state, "Restored from ledger.json.bak");
                    let payload = ledger::dump(&self.state);
                    match write_file(&paths.ledger, payload.as_bytes(), LEDGER_MAX) {
                        Ok(()) => {
                            self.signature = file_sig(&paths.ledger);
                            *wrote = true;
                        }
                        Err(error) => {
                            self.status = format!("Restored in memory but could not rewrite ledger: {}", error.message);
                        }
                    }
                }
                _ => self.seed_blocked("Ledger missing/damaged and backup unavailable. Showing empty until you save an entry."),
            },
            Err(_) => self.seed_blocked("Ledger missing/damaged and no backup found. Showing empty until you save an entry."),
        }
    }
}

pub fn read_file(path: &Path, max_bytes: usize) -> Result<Vec<u8>, IoError> {
    validate_max(max_bytes)?;
    let parts = canonical_parts(path)?;
    let uid = current_uid();
    let dir = open_directory_chain(&parts[..parts.len() - 1], uid, false)?;
    read_named(&dir, &parts[parts.len() - 1], max_bytes, uid)
}

pub fn write_file(path: &Path, data: &[u8], max_bytes: usize) -> Result<(), IoError> {
    validate_max(max_bytes)?;
    if data.len() > max_bytes {
        return Err(IoError::new(CODE_LIMIT, "payload too large"));
    }
    let parts = canonical_parts(path)?;
    let uid = current_uid();
    let dir = open_directory_chain(&parts[..parts.len() - 1], uid, true)?;
    atomic_replace(&dir, &parts[parts.len() - 1], data, uid)
}

pub fn save_file(path: &Path, backup: &Path, data: &[u8], max_bytes: usize) -> Result<(), IoError> {
    validate_max(max_bytes)?;
    if data.len() > max_bytes {
        return Err(IoError::new(CODE_LIMIT, "payload too large"));
    }
    let dest = canonical_parts(path)?;
    let bak = canonical_parts(backup)?;
    if dest[..dest.len() - 1] != bak[..bak.len() - 1] {
        return Err(IoError::new(
            CODE_USAGE,
            "backup must be in the same directory as the ledger",
        ));
    }
    let uid = current_uid();
    let dir = open_directory_chain(&dest[..dest.len() - 1], uid, true)?;
    if let Some(existing) = try_read_named(&dir, &dest[dest.len() - 1], max_bytes, uid)? {
        atomic_replace(&dir, &bak[bak.len() - 1], &existing, uid)?;
    }
    atomic_replace(&dir, &dest[dest.len() - 1], data, uid)
}

fn validate_max(max_bytes: usize) -> Result<(), IoError> {
    if (1..=MAX_ALLOWED).contains(&max_bytes) {
        Ok(())
    } else {
        Err(IoError::new(CODE_USAGE, "invalid max-bytes"))
    }
}

fn current_uid() -> u32 {
    unsafe { libc::geteuid() }
}

fn canonical_parts(path: &Path) -> Result<Vec<String>, IoError> {
    let text = path
        .to_str()
        .ok_or_else(|| IoError::new(CODE_USAGE, "invalid path"))?;
    if text.is_empty() || text.contains('\0') {
        return Err(IoError::new(CODE_USAGE, "invalid path"));
    }
    if text.len() > 4096 {
        return Err(IoError::new(CODE_USAGE, "path too long"));
    }
    if !text.starts_with('/') || text.ends_with('/') || text.contains("//") {
        return Err(IoError::new(
            CODE_USAGE,
            "path must be absolute and canonical",
        ));
    }
    let parts: Vec<String> = text.split('/').skip(1).map(str::to_string).collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(IoError::new(CODE_USAGE, "path must be canonical"));
    }
    if format!("/{}", parts.join("/")) != text {
        return Err(IoError::new(CODE_USAGE, "path must be canonical"));
    }
    if parts.is_empty() {
        return Err(IoError::new(CODE_USAGE, "path must be canonical"));
    }
    Ok(parts)
}

fn validate_component(name: &str) -> Result<(), IoError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        Err(IoError::new(CODE_USAGE, "invalid path component"))
    } else {
        Ok(())
    }
}

fn open_directory_chain(parts: &[String], uid: u32, create: bool) -> Result<OwnedFd, IoError> {
    let root = open_root()?;
    check_directory(&root, uid)?;
    let mut dir = root;
    for part in parts {
        validate_component(part)?;
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        match open_at(&dir, part, flags) {
            Ok(next) => {
                check_directory(&next, uid)?;
                dir = next;
            }
            Err(error) if error.code == CODE_MISSING && create => {
                let parent = fstat(&dir)?;
                if parent.st_uid as u32 != uid {
                    return Err(IoError::new(
                        CODE_PERM,
                        "refusing to create directory under non-owned parent",
                    ));
                }
                mkdir_at(&dir, part)?;
                let next = open_at(&dir, part, flags).map_err(|error| map_as_directory(error))?;
                check_directory(&next, uid)?;
                dir = next;
            }
            Err(error) => return Err(map_as_directory(error)),
        }
    }
    Ok(dir)
}

fn map_as_directory(error: IoError) -> IoError {
    if error.message == "unexpected file type" {
        IoError::new(CODE_TYPE, "not a directory")
    } else {
        error
    }
}

fn open_root() -> Result<OwnedFd, IoError> {
    let fd = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(IoError::new(CODE_ERR, "cannot open directory"));
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn open_at(dir: &OwnedFd, name: &str, flags: i32) -> Result<OwnedFd, IoError> {
    let c_name = std::ffi::CString::new(name)
        .map_err(|_| IoError::new(CODE_USAGE, "invalid path component"))?;
    loop {
        let fd = unsafe { libc::openat(dir.as_raw_fd(), c_name.as_ptr(), flags) };
        if fd >= 0 {
            return Ok(unsafe { OwnedFd::from_raw_fd(fd) });
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        return Err(map_open(error));
    }
}

fn map_open(error: std::io::Error) -> IoError {
    match error.raw_os_error() {
        Some(code) if code == libc::ELOOP => IoError::new(CODE_TYPE, "refusing to follow symlink"),
        Some(code) if code == libc::ENOTDIR || code == libc::EISDIR => {
            IoError::new(CODE_TYPE, "unexpected file type")
        }
        Some(code) if code == libc::ENOENT => IoError::new(CODE_MISSING, "not found"),
        _ => IoError::new(CODE_ERR, "cannot open path"),
    }
}

fn mkdir_at(dir: &OwnedFd, name: &str) -> Result<(), IoError> {
    let c_name = std::ffi::CString::new(name)
        .map_err(|_| IoError::new(CODE_USAGE, "invalid path component"))?;
    let rc = unsafe { libc::mkdirat(dir.as_raw_fd(), c_name.as_ptr(), 0o700) };
    if rc == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == ErrorKind::AlreadyExists {
        return Ok(());
    }
    Err(map_open(error))
}

fn check_directory(fd: &OwnedFd, uid: u32) -> Result<(), IoError> {
    let st = fstat(fd)?;
    if !is_type(st.st_mode, libc::S_IFDIR) {
        return Err(IoError::new(CODE_TYPE, "not a directory"));
    }
    if st.st_uid as u32 != 0 && st.st_uid as u32 != uid {
        return Err(IoError::new(CODE_PERM, "directory has unexpected owner"));
    }
    if mode_bits(st.st_mode) & libc::S_IWOTH as u32 != 0 {
        return Err(IoError::new(CODE_PERM, "directory is world-writable"));
    }
    Ok(())
}

fn read_named(dir: &OwnedFd, name: &str, max_bytes: usize, uid: u32) -> Result<Vec<u8>, IoError> {
    let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    let file = open_at(dir, name, flags)?;
    let st = fstat(&file)?;
    if !is_type(st.st_mode, libc::S_IFREG) {
        return Err(IoError::new(CODE_TYPE, "not a regular file"));
    }
    if st.st_uid as u32 != uid {
        return Err(IoError::new(CODE_PERM, "file has unexpected owner"));
    }
    let data = read_bounded(&file, max_bytes)?;
    let again = fstat(&file)?;
    if (st.st_dev, st.st_ino) != (again.st_dev, again.st_ino) {
        return Err(IoError::new(CODE_TYPE, "file identity changed"));
    }
    Ok(data)
}

fn try_read_named(
    dir: &OwnedFd,
    name: &str,
    max_bytes: usize,
    uid: u32,
) -> Result<Option<Vec<u8>>, IoError> {
    match read_named(dir, name, max_bytes, uid) {
        Ok(data) if data.is_empty() => Ok(None),
        Ok(data) => Ok(Some(data)),
        Err(error) if error.code == CODE_MISSING => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_bounded(fd: &OwnedFd, max_bytes: usize) -> Result<Vec<u8>, IoError> {
    let mut buf = Vec::new();
    let limit = max_bytes.saturating_add(1);
    let mut tmp = [0u8; 65536];
    while buf.len() < limit {
        let want = (limit - buf.len()).min(tmp.len());
        let n = unsafe {
            libc::read(
                fd.as_raw_fd(),
                tmp.as_mut_ptr().cast::<libc::c_void>(),
                want,
            )
        };
        if n < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(IoError::new(CODE_ERR, "cannot open path"));
        }
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n as usize]);
    }
    if buf.len() > max_bytes {
        return Err(IoError::new(CODE_LIMIT, "file exceeds size limit"));
    }
    Ok(buf)
}

fn refuse_bad_destination(dir: &OwnedFd, name: &str, uid: u32) -> Result<(), IoError> {
    validate_component(name)?;
    let c_name = std::ffi::CString::new(name)
        .map_err(|_| IoError::new(CODE_USAGE, "invalid path component"))?;
    let mut st = unsafe { std::mem::zeroed::<libc::stat>() };
    let rc = unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            c_name.as_ptr(),
            &mut st,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            return Ok(());
        }
        return Err(IoError::new(CODE_ERR, "cannot open path"));
    }
    if is_type(st.st_mode, libc::S_IFLNK) {
        return Err(IoError::new(CODE_TYPE, "refusing to replace a symlink"));
    }
    if !is_type(st.st_mode, libc::S_IFREG) {
        return Err(IoError::new(CODE_TYPE, "destination is not a regular file"));
    }
    if st.st_uid as u32 != uid {
        return Err(IoError::new(CODE_PERM, "destination has unexpected owner"));
    }
    Ok(())
}

fn atomic_replace(dir: &OwnedFd, name: &str, data: &[u8], uid: u32) -> Result<(), IoError> {
    refuse_bad_destination(dir, name, uid)?;
    let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    let mut temp_name: Option<std::ffi::CString> = None;
    let mut file: Option<OwnedFd> = None;
    for _ in 0..16 {
        let candidate = format!("{name}.tmp.{}", hex_token());
        let c_name = match std::ffi::CString::new(candidate) {
            Ok(name) => name,
            Err(_) => continue,
        };
        let fd = unsafe { libc::openat(dir.as_raw_fd(), c_name.as_ptr(), flags, 0o600) };
        if fd >= 0 {
            file = Some(unsafe { OwnedFd::from_raw_fd(fd) });
            temp_name = Some(c_name);
            break;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() == ErrorKind::AlreadyExists {
            continue;
        }
        if error.raw_os_error() == Some(libc::ELOOP) {
            return Err(IoError::new(CODE_TYPE, "refusing to follow symlink"));
        }
        return Err(IoError::new(CODE_ERR, "cannot create temporary file"));
    }
    let Some(file) = file else {
        return Err(IoError::new(
            CODE_ERR,
            "could not create exclusive temporary file",
        ));
    };
    let result = (|| {
        if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } < 0 {
            return Err(IoError::new(CODE_ERR, "cannot create temporary file"));
        }
        let st = fstat(&file)?;
        if !is_type(st.st_mode, libc::S_IFREG) || st.st_uid as u32 != uid {
            return Err(IoError::new(CODE_PERM, "temporary file failed validation"));
        }
        write_all(&file, data)?;
        if unsafe { libc::fsync(file.as_raw_fd()) } < 0 {
            return Err(IoError::new(CODE_ERR, "cannot create temporary file"));
        }
        Ok(())
    })();
    drop(file);
    if let Err(error) = result {
        unlink_temp(dir, temp_name.as_ref());
        return Err(error);
    }
    let Some(temp_name) = temp_name else {
        return Err(IoError::new(
            CODE_ERR,
            "could not create exclusive temporary file",
        ));
    };
    let dest = std::ffi::CString::new(name)
        .map_err(|_| IoError::new(CODE_USAGE, "invalid path component"))?;
    let rc = unsafe {
        libc::renameat(
            dir.as_raw_fd(),
            temp_name.as_ptr(),
            dir.as_raw_fd(),
            dest.as_ptr(),
        )
    };
    if rc < 0 {
        unlink_temp(dir, Some(&temp_name));
        return Err(IoError::new(CODE_ERR, "cannot create temporary file"));
    }
    if unsafe { libc::fsync(dir.as_raw_fd()) } < 0 {
        return Err(IoError::new(CODE_ERR, "cannot create temporary file"));
    }
    Ok(())
}

fn unlink_temp(dir: &OwnedFd, name: Option<&std::ffi::CString>) {
    if let Some(name) = name {
        unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) };
    }
}

fn write_all(fd: &OwnedFd, data: &[u8]) -> Result<(), IoError> {
    let mut written = 0;
    while written < data.len() {
        let n = unsafe {
            libc::write(
                fd.as_raw_fd(),
                data[written..].as_ptr().cast::<libc::c_void>(),
                data.len() - written,
            )
        };
        if n < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(IoError::new(CODE_ERR, "short write"));
        }
        if n == 0 {
            return Err(IoError::new(CODE_ERR, "short write"));
        }
        written += n as usize;
    }
    Ok(())
}

fn fstat(fd: &OwnedFd) -> Result<libc::stat, IoError> {
    let mut st = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } < 0 {
        return Err(IoError::new(CODE_ERR, "cannot open path"));
    }
    Ok(st)
}

fn is_type(mode: libc::mode_t, kind: libc::mode_t) -> bool {
    mode & libc::S_IFMT == kind
}

fn mode_bits(mode: libc::mode_t) -> u32 {
    mode as u32 & 0o777
}

fn hex_token() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = ledger::random_bytes(16);
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn file_sig(path: &Path) -> Option<FileSig> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    Some(FileSig {
        len: meta.len(),
        modified: meta.modified().ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("mybudget-{}", hex_token()));
            std::fs::create_dir(&dir).unwrap();
            let dir = std::fs::canonicalize(&dir).unwrap();
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(dir)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn read_write_round_trip_and_mode() {
        let tmp = Tmp::new();
        let path = tmp.path("ledger.json");
        let body = "{\"note\": \"caf\u{e9} \u{1f600}\"}\n".as_bytes();
        write_file(&path, body, 4096).unwrap();
        assert_eq!(read_file(&path, 4096).unwrap(), body);
        let mode = std::fs::symlink_metadata(&path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let leftovers: Vec<_> = std::fs::read_dir(&tmp.0)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn read_rejects_missing_oversize_symlink_directory_and_dotdot() {
        let tmp = Tmp::new();
        let missing = read_file(&tmp.path("missing.json"), 4096).unwrap_err();
        assert_eq!(missing.code, CODE_MISSING);
        let big = tmp.path("big.json");
        std::fs::write(&big, vec![b'x'; 64]).unwrap();
        let over = read_file(&big, 16).unwrap_err();
        assert_eq!(over.code, CODE_LIMIT);

        let secret = tmp.path("secret.json");
        std::fs::write(&secret, b"secret-bytes").unwrap();
        symlink(&secret, tmp.path("ledger.json")).unwrap();
        let linked = read_file(&tmp.path("ledger.json"), 4096).unwrap_err();
        assert_eq!(linked.code, CODE_TYPE);

        let outside = Tmp::new();
        std::fs::write(outside.path("ledger.json"), b"outside-bytes").unwrap();
        symlink(&outside.0, tmp.path("data")).unwrap();
        let via = read_file(&tmp.path("data").join("ledger.json"), 4096).unwrap_err();
        assert_eq!(via.code, CODE_TYPE);

        assert_eq!(read_file(&tmp.0, 4096).unwrap_err().code, CODE_TYPE);
        assert_eq!(
            read_file(&PathBuf::from(format!("{}/", tmp.0.display())), 4096)
                .unwrap_err()
                .code,
            CODE_USAGE
        );
        assert_eq!(
            read_file(Path::new("ledger.json"), 4096).unwrap_err().code,
            CODE_USAGE
        );
        assert_eq!(
            read_file(
                &PathBuf::from(format!("{}/../ledger.json", tmp.0.display())),
                4096
            )
            .unwrap_err()
            .code,
            CODE_USAGE
        );
    }

    #[test]
    fn write_refuses_symlink_and_oversize_payload() {
        let tmp = Tmp::new();
        let other = tmp.path("other.json");
        std::fs::write(&other, b"keep").unwrap();
        let dest = tmp.path("ledger.json");
        symlink(&other, &dest).unwrap();
        let error = write_file(&dest, b"new", 4096).unwrap_err();
        assert_eq!(error.code, CODE_TYPE);
        assert!(dest.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&other).unwrap(), b"keep");

        let fresh = tmp.path("fresh.json");
        assert_eq!(
            write_file(&fresh, b"12345", 4).unwrap_err().code,
            CODE_LIMIT
        );
        assert!(!fresh.exists());
    }

    #[test]
    fn save_backs_up_then_replaces_and_skips_empty_sources() {
        let tmp = Tmp::new();
        let path = tmp.path("ledger.json");
        let backup = tmp.path("ledger.json.bak");
        std::fs::write(&path, b"{\"old\": 1}\n").unwrap();
        save_file(&path, &backup, b"{\"new\": 2}\n", 4096).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), b"{\"old\": 1}\n");
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"new\": 2}\n");

        let nested = tmp.path("nested/ledger.json");
        let nested_bak = tmp.path("nested/ledger.json.bak");
        save_file(&nested, &nested_bak, b"{\"fresh\": true}\n", 4096).unwrap();
        assert!(!nested_bak.exists());
        assert_eq!(std::fs::read(&nested).unwrap(), b"{\"fresh\": true}\n");
        let mode = std::fs::symlink_metadata(tmp.path("nested"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);

        let empty = tmp.path("empty.json");
        let empty_bak = tmp.path("empty.json.bak");
        std::fs::File::create(&empty).unwrap();
        save_file(&empty, &empty_bak, b"{\"fresh\": true}\n", 4096).unwrap();
        assert!(!empty_bak.exists());

        let other_bak = tmp.path("other/ledger.json.bak");
        assert_eq!(
            save_file(&path, &other_bak, b"{}\n", 4096)
                .unwrap_err()
                .code,
            CODE_USAGE
        );
    }

    #[test]
    fn save_refuses_symlink_live_file_and_write_leaves_backup() {
        let tmp = Tmp::new();
        let path = tmp.path("ledger.json");
        let backup = tmp.path("ledger.json.bak");
        let other = tmp.path("other.json");
        std::fs::write(&other, b"{\"old\": 1}\n").unwrap();
        symlink(&other, &path).unwrap();
        assert_eq!(
            save_file(&path, &backup, b"{\"new\": 2}\n", 4096)
                .unwrap_err()
                .code,
            CODE_TYPE
        );
        assert!(!backup.exists());
        assert_eq!(std::fs::read(&other).unwrap(), b"{\"old\": 1}\n");

        std::fs::write(&backup, b"{\"good\": true}\n").unwrap();
        let live = tmp.path("live.json");
        write_file(&live, b"{\"restored\": true}\n", 4096).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), b"{\"good\": true}\n");
    }

    #[test]
    fn world_writable_directory_is_refused() {
        let tmp = Tmp::new();
        let world = tmp.path("world");
        std::fs::create_dir(&world).unwrap();
        std::fs::set_permissions(&world, std::fs::Permissions::from_mode(0o777)).unwrap();
        let path = world.join("ledger.json");
        std::fs::write(&path, b"nope").unwrap();
        let error = read_file(&path, 4096).unwrap_err();
        assert_eq!(error.code, CODE_PERM);
    }

    #[test]
    fn damaged_ledger_restores_from_backup_without_replacing_it() {
        let tmp = Tmp::new();
        let paths = Paths {
            ledger: tmp.path("ledger.json"),
            backup: tmp.path("ledger.json.bak"),
        };
        std::fs::write(&paths.ledger, b"{").unwrap();
        std::fs::write(&paths.backup, b"{\"entries\":[{\"id\":\"e1\",\"kind\":\"expense\",\"amountCents\":100,\"date\":\"2026-01-04\",\"title\":\"Cafe\"}]}").unwrap();
        let session = Session::open(Storage::Disk(paths.clone()));
        assert_eq!(session.status, "Restored from ledger.json.bak");
        assert_eq!(session.state.entries.len(), 1);
        assert!(!session.block_empty_persist);
        assert!(
            std::fs::read_to_string(&paths.ledger)
                .unwrap()
                .contains("Cafe")
        );
        assert!(
            std::fs::read_to_string(&paths.backup)
                .unwrap()
                .starts_with('{')
        );
        assert!(
            !std::fs::read_to_string(&paths.backup)
                .unwrap()
                .contains("Trying")
        );
    }

    #[test]
    fn missing_ledger_blocks_an_empty_save_until_there_is_an_entry() {
        let tmp = Tmp::new();
        let paths = Paths {
            ledger: tmp.path("ledger.json"),
            backup: tmp.path("ledger.json.bak"),
        };
        let mut session = Session::open(Storage::Disk(paths.clone()));
        assert!(session.block_empty_persist);
        assert!(session.status.contains("no backup found"));
        assert!(!session.persist());
        assert!(!paths.ledger.exists());
        session.state = ledger::add_entry(
            &session.state,
            ledger::NewEntry::manual(
                ledger::Kind::Expense,
                "2026-01-04".into(),
                100,
                "Cafe".into(),
                String::new(),
                "food".into(),
            ),
        );
        assert!(session.persist());
        assert!(paths.ledger.exists());
        assert!(!paths.backup.exists());
    }
}

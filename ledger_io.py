#!/usr/bin/python3
"""Bounded, no-follow, owner-checked ledger and QFX I/O for MyExpenses.

Reads use an O_NOFOLLOW descriptor, require a regular file owned by the
effective user, and stop at a byte limit while re-checking file identity.
Writes create an exclusive randomized temporary in the same directory, fsync
the payload, then atomically replace the destination and fsync the directory.
save() copies the live ledger to the backup through that path and only then
replaces the ledger.
"""

from __future__ import annotations

import argparse
import errno
import os
import secrets
import stat
import sys

EXIT_OK = 0
EXIT_ERR = 1
EXIT_MISSING = 2
EXIT_LIMIT = 3
EXIT_TYPE = 4
EXIT_PERM = 5
EXIT_USAGE = 6

MAX_PATH = 4096
MAX_ALLOWED_LIMIT = 64 * 1024 * 1024
TEMP_ATTEMPTS = 16


class IoError(Exception):
    def __init__(self, code: int, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


def validate_component(name: str) -> None:
    if not name or name in (".", "..") or "/" in name or "\x00" in name:
        raise IoError(EXIT_USAGE, "invalid path component")


def canonical_parts(path: str) -> list[str]:
    if not isinstance(path, str) or not path or "\x00" in path:
        raise IoError(EXIT_USAGE, "invalid path")
    if len(path) > MAX_PATH:
        raise IoError(EXIT_USAGE, "path too long")
    if not path.startswith("/") or path.endswith("/") or "//" in path:
        raise IoError(EXIT_USAGE, "path must be absolute and canonical")
    parts = path.split("/")[1:]
    if any(part in ("", ".", "..") for part in parts):
        raise IoError(EXIT_USAGE, "path must be canonical")
    if "/" + "/".join(parts) != path:
        raise IoError(EXIT_USAGE, "path must be canonical")
    return parts


def _map_open_error(exc: OSError, what: str) -> IoError:
    if exc.errno == errno.ELOOP:
        return IoError(EXIT_TYPE, "refusing to follow symlink")
    if exc.errno in (errno.ENOTDIR, errno.EISDIR):
        return IoError(EXIT_TYPE, "not a directory" if what == "directory" else "not a regular file")
    if exc.errno == errno.ENOENT:
        return IoError(EXIT_MISSING, what + " not found")
    return IoError(EXIT_ERR, "cannot open " + what)


def _open_nofollow(dirfd: int, name: str, flags: int, mode: int = 0) -> int:
    validate_component(name)
    try:
        return os.open(name, flags, mode, dir_fd=dirfd)
    except FileNotFoundError as exc:
        raise IoError(EXIT_MISSING, "not found") from exc
    except OSError as exc:
        if exc.errno == errno.ELOOP:
            raise IoError(EXIT_TYPE, "refusing to follow symlink") from exc
        if exc.errno in (errno.ENOTDIR, errno.EISDIR):
            raise IoError(EXIT_TYPE, "unexpected file type") from exc
        raise IoError(EXIT_ERR, "cannot open path") from exc


def _check_directory(fd: int, uid: int) -> None:
    st = os.fstat(fd)
    if not stat.S_ISDIR(st.st_mode):
        raise IoError(EXIT_TYPE, "not a directory")
    if st.st_uid not in (0, uid):
        raise IoError(EXIT_PERM, "directory has unexpected owner")
    if st.st_mode & stat.S_IWOTH:
        raise IoError(EXIT_PERM, "directory is world-writable")


def open_directory_chain(parts: list[str], uid: int, *, create: bool) -> int:
    fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    try:
        _check_directory(fd, uid)
        for part in parts:
            validate_component(part)
            flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
            try:
                next_fd = os.open(part, flags, dir_fd=fd)
            except FileNotFoundError:
                if not create:
                    raise IoError(EXIT_MISSING, "directory not found")
                parent = os.fstat(fd)
                if parent.st_uid != uid:
                    raise IoError(EXIT_PERM, "refusing to create directory under non-owned parent")
                try:
                    os.mkdir(part, 0o700, dir_fd=fd)
                except FileExistsError:
                    pass
                try:
                    next_fd = os.open(part, flags, dir_fd=fd)
                except OSError as exc:
                    raise _map_open_error(exc, "directory") from exc
            except OSError as exc:
                raise _map_open_error(exc, "directory") from exc
            try:
                _check_directory(next_fd, uid)
            except IoError:
                os.close(next_fd)
                raise
            os.close(fd)
            fd = next_fd
        return fd
    except Exception:
        os.close(fd)
        raise


def read_bounded(fd: int, max_bytes: int) -> bytes:
    buf = bytearray()
    limit = max_bytes + 1
    while len(buf) < limit:
        chunk = os.read(fd, min(65536, limit - len(buf)))
        if not chunk:
            break
        buf.extend(chunk)
    if len(buf) > max_bytes:
        raise IoError(EXIT_LIMIT, "file exceeds size limit")
    return bytes(buf)


def read_named(dirfd: int, name: str, max_bytes: int, uid: int) -> bytes:
    fd = _open_nofollow(dirfd, name, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode):
            raise IoError(EXIT_TYPE, "not a regular file")
        if st.st_uid != uid:
            raise IoError(EXIT_PERM, "file has unexpected owner")
        data = read_bounded(fd, max_bytes)
        st2 = os.fstat(fd)
        if (st.st_dev, st.st_ino) != (st2.st_dev, st2.st_ino):
            raise IoError(EXIT_TYPE, "file identity changed")
        return data
    finally:
        os.close(fd)


def try_read_named(dirfd: int, name: str, max_bytes: int, uid: int) -> bytes | None:
    try:
        data = read_named(dirfd, name, max_bytes, uid)
    except IoError as exc:
        if exc.code == EXIT_MISSING:
            return None
        raise
    return data or None


def _refuse_bad_destination(dirfd: int, name: str, uid: int) -> None:
    validate_component(name)
    try:
        st = os.stat(name, dir_fd=dirfd, follow_symlinks=False)
    except FileNotFoundError:
        return
    if stat.S_ISLNK(st.st_mode):
        raise IoError(EXIT_TYPE, "refusing to replace a symlink")
    if not stat.S_ISREG(st.st_mode):
        raise IoError(EXIT_TYPE, "destination is not a regular file")
    if st.st_uid != uid:
        raise IoError(EXIT_PERM, "destination has unexpected owner")


def atomic_replace(dirfd: int, name: str, data: bytes, uid: int) -> None:
    _refuse_bad_destination(dirfd, name, uid)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
    tmp_name = None
    fd = None
    try:
        for _ in range(TEMP_ATTEMPTS):
            candidate = name + ".tmp." + secrets.token_hex(16)
            try:
                fd = os.open(candidate, flags, 0o600, dir_fd=dirfd)
                tmp_name = candidate
                break
            except FileExistsError:
                continue
            except OSError as exc:
                if exc.errno == errno.ELOOP:
                    raise IoError(EXIT_TYPE, "refusing to follow symlink") from exc
                raise IoError(EXIT_ERR, "cannot create temporary file") from exc
        if fd is None or tmp_name is None:
            raise IoError(EXIT_ERR, "could not create exclusive temporary file")
        os.fchmod(fd, 0o600)
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode) or st.st_uid != uid:
            raise IoError(EXIT_PERM, "temporary file failed validation")
        written = 0
        view = memoryview(data)
        while written < len(data):
            n = os.write(fd, view[written:])
            if n <= 0:
                raise IoError(EXIT_ERR, "short write")
            written += n
        os.fsync(fd)
        os.close(fd)
        fd = None
        os.rename(tmp_name, name, src_dir_fd=dirfd, dst_dir_fd=dirfd)
        tmp_name = None
        os.fsync(dirfd)
    finally:
        if fd is not None:
            os.close(fd)
        if tmp_name is not None:
            try:
                os.unlink(tmp_name, dir_fd=dirfd)
            except OSError:
                pass


def read_payload(max_bytes: int) -> bytes:
    header = sys.stdin.buffer.readline()
    if not header:
        raise IoError(EXIT_USAGE, "missing payload length")
    try:
        n = int(header.strip())
    except ValueError as exc:
        raise IoError(EXIT_USAGE, "invalid payload length") from exc
    if n < 0 or n > max_bytes:
        raise IoError(EXIT_LIMIT, "payload too large")
    data = bytearray()
    while len(data) < n:
        chunk = sys.stdin.buffer.read(n - len(data))
        if not chunk:
            raise IoError(EXIT_USAGE, "short payload")
        data.extend(chunk)
    return bytes(data)


def cmd_read(path: str, max_bytes: int) -> None:
    uid = os.geteuid()
    parts = canonical_parts(path)
    dirfd = open_directory_chain(parts[:-1], uid, create=False)
    try:
        data = read_named(dirfd, parts[-1], max_bytes, uid)
    finally:
        os.close(dirfd)
    sys.stdout.buffer.write(data)
    sys.stdout.buffer.flush()


def cmd_write(path: str, max_bytes: int, payload: bytes) -> None:
    if len(payload) > max_bytes:
        raise IoError(EXIT_LIMIT, "payload too large")
    uid = os.geteuid()
    parts = canonical_parts(path)
    dirfd = open_directory_chain(parts[:-1], uid, create=True)
    try:
        atomic_replace(dirfd, parts[-1], payload, uid)
    finally:
        os.close(dirfd)


def cmd_save(path: str, backup: str, max_bytes: int, payload: bytes) -> None:
    if len(payload) > max_bytes:
        raise IoError(EXIT_LIMIT, "payload too large")
    uid = os.geteuid()
    dest_parts = canonical_parts(path)
    backup_parts = canonical_parts(backup)
    if dest_parts[:-1] != backup_parts[:-1]:
        raise IoError(EXIT_USAGE, "backup must be in the same directory as the ledger")
    dirfd = open_directory_chain(dest_parts[:-1], uid, create=True)
    try:
        existing = try_read_named(dirfd, dest_parts[-1], max_bytes, uid)
        if existing is not None:
            atomic_replace(dirfd, backup_parts[-1], existing, uid)
        atomic_replace(dirfd, dest_parts[-1], payload, uid)
    finally:
        os.close(dirfd)


def verify_self() -> None:
    path = __file__
    if not os.path.isabs(path):
        raise IoError(EXIT_ERR, "helper path is not absolute")
    flags = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW
    try:
        fd = os.open(path, flags)
    except OSError as exc:
        raise IoError(EXIT_ERR, "helper is not a regular file") from exc
    try:
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode):
            raise IoError(EXIT_ERR, "helper is not a regular file")
        if st.st_uid not in (0, os.geteuid()):
            raise IoError(EXIT_ERR, "helper has unexpected owner")
        if st.st_mode & 0o022:
            raise IoError(EXIT_ERR, "helper is writable by group or other")
    finally:
        os.close(fd)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(prog="ledger_io.py")
    sub = parser.add_subparsers(dest="cmd", required=True)
    reader = sub.add_parser("read")
    reader.add_argument("--path", required=True)
    reader.add_argument("--max-bytes", type=int, required=True)
    writer = sub.add_parser("write")
    writer.add_argument("--path", required=True)
    writer.add_argument("--max-bytes", type=int, required=True)
    saver = sub.add_parser("save")
    saver.add_argument("--path", required=True)
    saver.add_argument("--backup", required=True)
    saver.add_argument("--max-bytes", type=int, required=True)
    args = parser.parse_args(argv)
    if args.max_bytes < 1 or args.max_bytes > MAX_ALLOWED_LIMIT:
        raise IoError(EXIT_USAGE, "invalid max-bytes")
    return args


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    try:
        verify_self()
        args = parse_args(argv)
        if args.cmd == "read":
            cmd_read(args.path, args.max_bytes)
        elif args.cmd == "write":
            cmd_write(args.path, args.max_bytes, read_payload(args.max_bytes))
        elif args.cmd == "save":
            cmd_save(args.path, args.backup, args.max_bytes, read_payload(args.max_bytes))
        else:
            raise IoError(EXIT_USAGE, "unknown command")
        return EXIT_OK
    except IoError as exc:
        sys.stderr.buffer.write((exc.message + "\n").encode("utf-8", "replace"))
        return exc.code
    except BrokenPipeError:
        return EXIT_ERR
    except Exception:
        sys.stderr.buffer.write(b"internal error\n")
        return EXIT_ERR


if __name__ == "__main__":
    sys.exit(main())

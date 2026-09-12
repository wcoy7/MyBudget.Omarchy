#!/usr/bin/env python3
"""Defensive tests for bounded, no-follow ledger I/O."""

from __future__ import annotations

import os
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
HELPER = os.path.join(ROOT, "ledger_io.py")
SANDBOX_ROOT = os.path.join(os.path.dirname(__file__), ".tmp")
CLOSED_ENV = {
    "LC_ALL": "C",
    "PYTHONIOENCODING": "utf-8",
}


def payload(data: bytes) -> bytes:
    return str(len(data)).encode("ascii") + b"\n" + data


class LedgerIoTests(unittest.TestCase):
    def setUp(self) -> None:
        os.makedirs(SANDBOX_ROOT, 0o700, exist_ok=True)
        self.sandbox = tempfile.mkdtemp(prefix="case-", dir=SANDBOX_ROOT)

    def tearDown(self) -> None:
        for dirpath, dirnames, filenames in os.walk(self.sandbox, topdown=False):
            for name in filenames:
                try:
                    os.unlink(os.path.join(dirpath, name))
                except OSError:
                    pass
            for name in dirnames:
                try:
                    os.rmdir(os.path.join(dirpath, name))
                except OSError:
                    pass
        try:
            os.rmdir(self.sandbox)
        except OSError:
            pass

    def run_helper(self, args, stdin=None, env=None):
        return subprocess.run(
            [sys.executable, "-I", "-B", HELPER, *args],
            input=stdin,
            capture_output=True,
            env=CLOSED_ENV if env is None else env,
            cwd="/",
            check=False,
        )

    def test_read_regular_owned_file(self) -> None:
        path = os.path.join(self.sandbox, "ledger.json")
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        try:
            os.write(fd, b'{"ok": true}\n')
        finally:
            os.close(fd)
        result = self.run_helper(["read", "--path", path, "--max-bytes", "4096"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b'{"ok": true}\n')

    def test_read_rejects_missing_file(self) -> None:
        path = os.path.join(self.sandbox, "missing.json")
        result = self.run_helper(["read", "--path", path, "--max-bytes", "4096"])
        self.assertEqual(result.returncode, 2)

    def test_read_rejects_oversize(self) -> None:
        path = os.path.join(self.sandbox, "big.json")
        with open(path, "wb") as handle:
            handle.write(b"x" * 64)
        result = self.run_helper(["read", "--path", path, "--max-bytes", "16"])
        self.assertEqual(result.returncode, 3)
        self.assertNotIn(b"xxxx", result.stdout)

    def test_read_rejects_file_symlink(self) -> None:
        secret = os.path.join(self.sandbox, "secret.json")
        visible = os.path.join(self.sandbox, "ledger.json")
        with open(secret, "wb") as handle:
            handle.write(b"secret-bytes")
        os.symlink(secret, visible)
        result = self.run_helper(["read", "--path", visible, "--max-bytes", "4096"])
        self.assertEqual(result.returncode, 4)
        self.assertNotIn(b"secret-bytes", result.stdout)

    def test_read_rejects_directory_symlink(self) -> None:
        outside = tempfile.mkdtemp(prefix="outside-", dir=SANDBOX_ROOT)
        try:
            target = os.path.join(outside, "ledger.json")
            with open(target, "wb") as handle:
                handle.write(b"outside-bytes")
            linkdir = os.path.join(self.sandbox, "data")
            os.symlink(outside, linkdir)
            result = self.run_helper(
                ["read", "--path", os.path.join(linkdir, "ledger.json"), "--max-bytes", "4096"]
            )
            self.assertEqual(result.returncode, 4)
            self.assertNotIn(b"outside-bytes", result.stdout)
        finally:
            try:
                os.unlink(os.path.join(outside, "ledger.json"))
                os.rmdir(outside)
            except OSError:
                pass

    def test_read_rejects_directory(self) -> None:
        result = self.run_helper(["read", "--path", self.sandbox, "--max-bytes", "4096"])
        self.assertEqual(result.returncode, 4)
        slashed = self.run_helper(["read", "--path", self.sandbox + "/", "--max-bytes", "4096"])
        self.assertEqual(slashed.returncode, 6)

    def test_read_rejects_relative_and_dotdot(self) -> None:
        rel = self.run_helper(["read", "--path", "ledger.json", "--max-bytes", "4096"])
        self.assertEqual(rel.returncode, 6)
        traversal = self.run_helper(
            ["read", "--path", self.sandbox + "/../ledger.json", "--max-bytes", "4096"]
        )
        self.assertEqual(traversal.returncode, 6)

    def test_write_atomic_replace_and_mode(self) -> None:
        path = os.path.join(self.sandbox, "ledger.json")
        body = b'{"entries": [], "note": "caf\xc3\xa9 \xf0\x9f\x98\x80"}\n'
        result = self.run_helper(
            ["write", "--path", path, "--max-bytes", "4096"],
            stdin=payload(body),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        with open(path, "rb") as handle:
            self.assertEqual(handle.read(), body)
        self.assertEqual(stat.S_IMODE(os.stat(path).st_mode), 0o600)
        leftovers = [name for name in os.listdir(self.sandbox) if ".tmp." in name]
        self.assertEqual(leftovers, [])

    def test_write_refuses_symlink_destination(self) -> None:
        dest = os.path.join(self.sandbox, "ledger.json")
        other = os.path.join(self.sandbox, "other.json")
        with open(other, "wb") as handle:
            handle.write(b"keep")
        os.symlink(other, dest)
        result = self.run_helper(
            ["write", "--path", dest, "--max-bytes", "4096"],
            stdin=payload(b"new"),
        )
        self.assertEqual(result.returncode, 4)
        self.assertTrue(os.path.islink(dest))
        with open(other, "rb") as handle:
            self.assertEqual(handle.read(), b"keep")

    def test_save_backs_up_then_replaces(self) -> None:
        path = os.path.join(self.sandbox, "ledger.json")
        backup = os.path.join(self.sandbox, "ledger.json.bak")
        with open(path, "wb") as handle:
            handle.write(b'{"old": 1}\n')
        result = self.run_helper(
            ["save", "--path", path, "--backup", backup, "--max-bytes", "4096"],
            stdin=payload(b'{"new": 2}\n'),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        with open(backup, "rb") as handle:
            self.assertEqual(handle.read(), b'{"old": 1}\n')
        with open(path, "rb") as handle:
            self.assertEqual(handle.read(), b'{"new": 2}\n')

    def test_save_skips_backup_when_source_missing(self) -> None:
        path = os.path.join(self.sandbox, "nested", "ledger.json")
        backup = os.path.join(self.sandbox, "nested", "ledger.json.bak")
        result = self.run_helper(
            ["save", "--path", path, "--backup", backup, "--max-bytes", "4096"],
            stdin=payload(b'{"fresh": true}\n'),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(os.path.exists(backup))
        with open(path, "rb") as handle:
            self.assertEqual(handle.read(), b'{"fresh": true}\n')
        mode = stat.S_IMODE(os.stat(os.path.join(self.sandbox, "nested")).st_mode)
        self.assertEqual(mode, 0o700)

    def test_save_skips_backup_when_source_empty(self) -> None:
        path = os.path.join(self.sandbox, "ledger.json")
        backup = os.path.join(self.sandbox, "ledger.json.bak")
        os.close(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600))
        result = self.run_helper(
            ["save", "--path", path, "--backup", backup, "--max-bytes", "4096"],
            stdin=payload(b'{"fresh": true}\n'),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(os.path.exists(backup))

    def test_save_requires_same_directory_backup(self) -> None:
        path = os.path.join(self.sandbox, "ledger.json")
        backup = os.path.join(self.sandbox, "other", "ledger.json.bak")
        result = self.run_helper(
            ["save", "--path", path, "--backup", backup, "--max-bytes", "4096"],
            stdin=payload(b"{}\n"),
        )
        self.assertEqual(result.returncode, 6)

    def test_save_refuses_symlink_live_file(self) -> None:
        path = os.path.join(self.sandbox, "ledger.json")
        backup = os.path.join(self.sandbox, "ledger.json.bak")
        other = os.path.join(self.sandbox, "other.json")
        with open(other, "wb") as handle:
            handle.write(b'{"old": 1}\n')
        os.symlink(other, path)
        result = self.run_helper(
            ["save", "--path", path, "--backup", backup, "--max-bytes", "4096"],
            stdin=payload(b'{"new": 2}\n'),
        )
        self.assertEqual(result.returncode, 4)
        self.assertFalse(os.path.exists(backup))
        with open(other, "rb") as handle:
            self.assertEqual(handle.read(), b'{"old": 1}\n')

    def test_write_does_not_touch_backup(self) -> None:
        path = os.path.join(self.sandbox, "ledger.json")
        backup = os.path.join(self.sandbox, "ledger.json.bak")
        with open(backup, "wb") as handle:
            handle.write(b'{"good": true}\n')
        result = self.run_helper(
            ["write", "--path", path, "--max-bytes", "4096"],
            stdin=payload(b'{"restored": true}\n'),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        with open(backup, "rb") as handle:
            self.assertEqual(handle.read(), b'{"good": true}\n')

    def test_closed_environment_does_not_need_path(self) -> None:
        path = os.path.join(self.sandbox, "ledger.json")
        with open(path, "wb") as handle:
            handle.write(b"abc")
        env = {
            "LC_ALL": "C",
            "PYTHONIOENCODING": "utf-8",
            "PATH": "/nonexistent",
            "PYTHONPATH": "/nonexistent",
            "LD_PRELOAD": "/nonexistent/preload.so",
        }
        result = self.run_helper(["read", "--path", path, "--max-bytes", "16"], env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b"abc")

    def test_rejects_world_writable_directory(self) -> None:
        world = os.path.join(self.sandbox, "world")
        os.mkdir(world, 0o777)
        os.chmod(world, 0o777)
        path = os.path.join(world, "ledger.json")
        with open(path, "wb") as handle:
            handle.write(b"nope")
        result = self.run_helper(["read", "--path", path, "--max-bytes", "4096"])
        self.assertEqual(result.returncode, 5)
        self.assertNotEqual(result.stdout, b"nope")

    def test_helper_refuses_symlink_self(self) -> None:
        link = os.path.join(self.sandbox, "ledger_io.py")
        os.symlink(HELPER, link)
        result = subprocess.run(
            [sys.executable, "-I", "-B", link, "read", "--path", os.path.join(self.sandbox, "x"), "--max-bytes", "16"],
            capture_output=True,
            env=CLOSED_ENV,
            cwd="/",
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"helper", result.stderr)

    def test_payload_over_limit(self) -> None:
        path = os.path.join(self.sandbox, "ledger.json")
        result = self.run_helper(
            ["write", "--path", path, "--max-bytes", "4"],
            stdin=payload(b"12345"),
        )
        self.assertEqual(result.returncode, 3)
        self.assertFalse(os.path.exists(path))


if __name__ == "__main__":
    unittest.main()

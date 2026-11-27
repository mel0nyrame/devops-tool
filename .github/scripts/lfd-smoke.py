"""Run scoped lfd CLI scenarios through a native macOS pseudoterminal."""

import errno
import fcntl
import os
from pathlib import Path
import plistlib
import pty
import re
import runpy
import select
import shlex
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unittest


# Use the same Ratatui screen reader as the rmem smoke check.
RATATUI_PTY = runpy.run_path(str(Path(__file__).with_name("rmem-smoke.py")))
Screen = RATATUI_PTY["Screen"]
ROWS, COLS = RATATUI_PTY["ROWS"], RATATUI_PTY["COLS"]


class Session:
    def __init__(self, binary, home, fixture_root):
        self.screen = Screen()
        self.transcript = bytearray()
        self.shell, self.fd = pty.fork()
        if self.shell == 0:
            os.environ.update(
                TERM="xterm-256color", PS1="lfd-smoke> ", ENV="",
                HOME=str(home), LFD_TEST_ROOT=str(fixture_root),
            )
            os.execl("/bin/sh", "sh", "-i")
        try:
            fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
            self.expect(r"lfd-smoke> ", terminal=True)
            self.original_settings = termios.tcgetattr(self.fd)
            self.send((shlex.quote(binary) + "; printf 'lfd-exit:%s\\n' \"$?\"\n").encode())
            self.expect("Application name or Bundle ID")
        except BaseException:
            self.close()
            raise

    def send(self, data):
        os.write(self.fd, data)

    def search(self, query):
        self.send(query.encode() + b"\r")

    def expect(self, pattern, *, terminal=False):
        deadline = time.monotonic() + 10
        while True:
            content = self.transcript.decode(errors="replace") if terminal else self.screen.text()
            if re.search(pattern, content):
                return
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.fd], [], [], remaining)[0]:
                break
            try:
                chunk = os.read(self.fd, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break
            if not chunk:
                break
            self.transcript.extend(chunk)
            self.screen.feed(chunk)
        raw = self.transcript.decode(errors="replace") if not self.screen.active else ""
        raise AssertionError(f"Expected {pattern!r}\n{content.rstrip()}\n{raw[-2000:]}")

    def quit(self, key=b"q"):
        self.send(key)
        self.expect(r"lfd-exit:0\r?\n[\s\S]*?lfd-smoke> ", terminal=True)
        assert not self.screen.active, "alternate screen was not restored"
        assert termios.tcgetattr(self.fd) == self.original_settings, "terminal settings changed"
        self.send(b"printf '%s%s\\n' lfd-shell- restored\n")
        self.expect(r"lfd-shell-restored\r?\n", terminal=True)

    def close(self):
        os.close(self.fd)
        try:
            os.kill(self.shell, signal.SIGKILL)
        except ProcessLookupError:
            pass
        os.waitpid(self.shell, 0)


def make_app(directory, filename, name, bundle_id, *, binary_plist=False):
    bundle = directory / filename
    info = bundle / "Contents" / "Info.plist"
    info.parent.mkdir(parents=True)
    info.write_bytes(plistlib.dumps(
        {"CFBundleDisplayName": name, "CFBundleIdentifier": bundle_id},
        fmt=plistlib.FMT_BINARY if binary_plist else plistlib.FMT_XML,
    ))
    return bundle


class IsolatedTests(unittest.TestCase):
    def setUp(self):
        fixture = tempfile.TemporaryDirectory(prefix="lfd-", dir="/private/tmp")
        self.addCleanup(fixture.cleanup)
        self.root = Path(fixture.name).resolve()
        self.home = self.root / "home"
        self.home.mkdir()

    def start(self):
        session = Session(BINARY, self.home, self.root)
        self.addCleanup(session.close)
        return session


class FixtureTests(IsolatedTests):
    def test_rejects_blank_and_path_queries_and_restores_terminal(self):
        session = self.start()
        session.search("  ")
        session.expect("Enter a non-empty application name or Bundle ID")
        session.send(b"\x7f\x7f")
        session.search("/Applications/Example.app")
        session.expect("not a path")
        session.quit(b"\x1b")

    def test_reports_no_match_without_searching_outside_the_fixed_roots(self):
        (self.home / "Scope Finder").mkdir()
        (self.home / "Library" / "Caches" / "nested" / "Scope Finder").mkdir(parents=True)
        make_app(self.home / "Applications" / "Nested", "Outside.app", "Scope Finder", "org.lfd.outside")
        separate_home = self.root / "unused-home"
        decoy = separate_home / "Library" / "Caches" / "Scope Finder"
        decoy.mkdir(parents=True)
        session = Session(BINARY, separate_home, self.root)
        self.addCleanup(session.close)
        session.search("Scope Finder")
        session.expect("No matching items")
        session.expect("Main program not found in the checked locations")
        session.quit()
        self.assertTrue(decoy.is_dir())

    def test_exact_bundle_id_uses_local_metadata_and_labels_related_items(self):
        apps = self.root / "System" / "Applications" / "Utilities"
        bundle = make_app(apps, "Internal.app", "Named Tool", "org.lfd.named", binary_plist=True)
        make_app(apps, "Other.app", "Other Tool", "org.lfd.named.extra")
        preference = self.home / "Library" / "Preferences" / "org.lfd.named.plist"
        preference.parent.mkdir(parents=True)
        preference.write_text("isolated preferences")
        session = self.start()
        session.search("org.lfd.named")
        session.expect("Main program found")
        session.expect("Related items")
        session.expect(re.escape(str(bundle)))
        session.expect(r"CFBundleIdentifier: org\.lfd\.named(?![.\w-])")
        self.assertNotIn("Other.app", session.screen.text())
        session.send(b"\x1b[B")
        session.expect(re.escape(str(preference)))
        session.expect("Preferences")
        session.expect("Risk: may contain retained settings")
        session.quit()
        self.assertEqual(preference.read_text(), "isolated preferences")

    def test_chooses_one_same_named_application_without_merging_artifacts(self):
        first = make_app(self.root / "Applications", "North.app", "Twin Tool", "org.lfd.north")
        second = make_app(self.home / "Applications", "South.app", "Twin Tool", "org.lfd.south")
        caches = self.home / "Library" / "Caches"
        for name in ("org.lfd.north", "org.lfd.south", "Twin Tool"):
            (caches / name).mkdir(parents=True)
        session = self.start()
        session.search("twin tool")
        session.expect("Choose one application")
        for text in ("org.lfd.north", "org.lfd.south", str(first), str(second)):
            session.expect(re.escape(text))
        session.send(b"\x1b[B\r")
        session.expect("Related items")
        session.expect(re.escape(str(second)))
        self.assertNotIn(str(first), session.screen.text())
        self.assertNotIn("org.lfd.north", session.screen.text())
        self.assertNotRegex(session.screen.text(), r"│ {1,3}Twin Tool +│")
        session.send(b"\x1b[B")
        session.expect(re.escape(str(caches / "org.lfd.south")))
        session.expect("Filename matches the local CFBundleIdentifier")
        session.quit()

    def test_reports_failed_application_checks_separately_from_matching_items(self):
        blocked_apps = self.home / "Applications"
        blocked_apps.write_text("not a directory")
        cache = self.home / "Library" / "Caches" / "Partial Tool"
        cache.mkdir(parents=True)
        session = self.start()
        session.search("Partial Tool")
        session.expect("Main program check incomplete")
        session.expect(re.escape(str(cache)))
        session.expect("Filename match only")
        session.expect("Uncheckable locations: 1")
        self.assertNotIn(str(blocked_apps), session.screen.text())
        session.send(b"i")
        session.expect("Could not check")
        session.expect(re.escape(str(blocked_apps)))
        session.expect("Not a directory")
        session.send(b"i")
        session.expect(re.escape(str(cache)))
        session.quit()

    def test_uses_readable_metadata_keys_when_an_optional_name_is_invalid(self):
        bundle = make_app(self.home / "Applications", "Different.app", 42, "org.lfd.readable")
        session = self.start()
        session.search("org.lfd.readable")
        session.expect("Main program found")
        session.expect(re.escape(str(bundle)))
        session.expect("CFBundleIdentifier: org.lfd.readable")
        session.send(b"i")
        session.expect("Expected strings in CFBundleDisplayName")
        session.quit()

    def test_skips_symlink_targets_and_keeps_unverified_filename_evidence(self):
        external = self.root / "outside"
        bundle = make_app(external, "Outside.app", "Redirected", "org.lfd.redirect")
        linked = self.home / "Applications" / "Link.app" / "Contents"
        linked.parent.mkdir(parents=True)
        linked.symlink_to(bundle / "Contents")
        outside_data = external / "org.lfd.redirect"
        outside_data.write_text("original")
        library = self.home / "Library"
        cache = library / "Caches" / "org.lfd.redirect"
        cache.parent.mkdir(parents=True)
        cache.symlink_to(outside_data)
        (library / "Application Support").symlink_to(external)
        session = self.start()
        session.search("org.lfd.redirect")
        session.expect("Main program check incomplete")
        session.expect("Uncheckable locations: 2")
        session.expect(re.escape(str(cache)))
        session.expect("Symbolic link: target not inspected")
        session.expect("Filename match only; application ownership not established")
        self.assertNotIn("Application data", session.screen.text())
        session.send(b"i")
        session.expect(re.escape(str(linked / "Info.plist")))
        session.expect("symbolic link not followed")
        session.send(b"\x1b[B")
        session.expect(re.escape(str(library / "Application Support")))
        session.expect("symbolic link not followed")
        session.quit()
        self.assertTrue(cache.is_symlink())
        self.assertEqual(outside_data.read_text(), "original")

    def test_reports_a_leftover_candidate_when_only_user_data_check_fails(self):
        library = self.home / "Library"
        candidate = library / "Application Support" / "org.lfd.absent"
        candidate.mkdir(parents=True)
        (library / "Caches").write_text("not a directory")
        session = self.start()
        session.search("org.lfd.absent")
        session.expect("Leftover candidates")
        session.expect("Main program not found in the checked locations")
        session.expect("This does not prove uninstallation")
        session.expect("Uncheckable locations: 1")
        session.expect(re.escape(str(candidate)))
        session.expect("Filename match only; application ownership not established")
        session.expect("Risk: may contain retained user data")
        session.quit()

    def test_uses_a_bundle_directory_name_when_metadata_is_missing(self):
        bundle = self.home / "Applications" / "Unrecorded.app"
        bundle.mkdir(parents=True)
        session = self.start()
        session.search("unrecorded")
        session.expect("Related items")
        session.expect(re.escape(str(bundle)))
        session.expect("Matched on bundle directory name")
        session.expect("Uncheckable locations: 1")
        session.send(b"i")
        session.expect(re.escape(str(bundle / "Contents" / "Info.plist")))
        session.expect("No such file or directory")
        session.quit()

    def test_fixture_build_requires_an_existing_absolute_root_and_home(self):
        no_home = self.root / "no-home"
        no_home.mkdir()
        linked_root = self.root / "linked-root"
        linked_root.symlink_to(self.root, target_is_directory=True)
        for root in (None, "relative", self.root / "missing", no_home, linked_root, "/"):
            with self.subTest(root=root):
                env = dict(os.environ, HOME=str(self.home))
                env.pop("LFD_TEST_ROOT", None)
                if root is not None:
                    env["LFD_TEST_ROOT"] = str(root)
                result = subprocess.run([BINARY], env=env, capture_output=True, text=True, timeout=5)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("LFD_TEST_ROOT", result.stderr)
                self.assertNotIn("Application name or Bundle ID", result.stdout)


class DefaultBuildTests(IsolatedTests):
    def test_rejects_arguments_instead_of_searching_an_arbitrary_path(self):
        result = subprocess.run(
            [BINARY, "/Applications/Example.app"], capture_output=True, text=True, timeout=5,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("lfd does not take arguments", result.stderr)

    def test_ignores_fixture_override(self):
        name = "lfd Smoke Verification 7938A51"
        make_app(self.root / "Applications", "Fixture Only.app", name, "org.lfd.fixtureonly")
        ordinary_home = self.root / "ordinary-home"
        cache = ordinary_home / "Library" / "Caches" / name
        cache.mkdir(parents=True)
        session = Session(BINARY, ordinary_home, self.root)
        self.addCleanup(session.close)
        session.search(name)
        session.expect(re.escape(str(cache)))
        session.expect("Filename match only")
        self.assertNotIn("Fixture Only.app", session.screen.text())
        session.quit()


if __name__ == "__main__":
    BINARY = os.path.abspath(sys.argv[1])
    tests = sys.argv[2:] or ["FixtureTests"]
    unittest.main(argv=[sys.argv[0], *tests], verbosity=2)

"""Run scoped lfd CLI scenarios through a native macOS pseudoterminal."""

import errno
import fcntl
import json
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
    def __init__(self, binary, home, fixture_root, path_dirs=()):
        self.screen = Screen()
        self.transcript = bytearray()
        self.shell, self.fd = pty.fork()
        if self.shell == 0:
            os.environ.update(
                TERM="xterm-256color", PS1="lfd-smoke> ", ENV="",
                HOME=str(home), LFD_TEST_ROOT=str(fixture_root),
                PATH=os.pathsep.join(map(str, path_dirs)),
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


def make_receipt(root, package_id, name, prefix="/"):
    receipt = root / "private" / "var" / "db" / "receipts" / f"{package_id}.plist"
    receipt.parent.mkdir(parents=True, exist_ok=True)
    receipt.write_bytes(plistlib.dumps({
        "PackageIdentifier": package_id,
        "PackageFileName": f"{name}.pkg",
        "InstallPrefixPath": prefix,
    }))
    return receipt


def make_receipt_bom(receipt, paths):
    with tempfile.TemporaryDirectory(dir="/private/tmp") as directory:
        listing = Path(directory) / "paths"
        listing.write_text(".\n" + "".join(f"./{path}\n" for path in paths))
        bom = receipt.with_suffix(".bom")
        subprocess.run(["/usr/bin/mkbom", "-s", "-i", str(listing), str(bom)], check=True)
        return bom


class IsolatedTests(unittest.TestCase):
    def setUp(self):
        fixture = tempfile.TemporaryDirectory(prefix="lfd-", dir="/private/tmp")
        self.addCleanup(fixture.cleanup)
        self.root = Path(fixture.name).resolve()
        self.home = self.root / "home"
        self.home.mkdir()

    def start(self, *path_dirs):
        session = Session(BINARY, self.home, self.root, path_dirs)
        self.addCleanup(session.close)
        return session

    def register_absent_bundle(self, bundle_id):
        # OS-boundary responses; neither file identifies an artifact or grants eligibility.
        records = self.root / "var" / "db" / "lfd-fixtures"
        records.mkdir(parents=True, exist_ok=True)
        (records / f"{bundle_id}.plist").write_bytes(plistlib.dumps({
            "CFBundleIdentifier": bundle_id,
        }))
        (records / f"{bundle_id}.checks").write_text(
            "PATH: not found\nHomebrew: not found\nPackage receipts: not found\n"
        )
        return records


class FixtureTests(IsolatedTests):
    def test_previews_selected_data_despite_an_uncheckable_cache_location(self):
        bundle_id = "org.lfd.preview"
        self.register_absent_bundle(bundle_id)
        library = self.home / "Library"
        candidate = library / "Application Support" / bundle_id
        candidate.mkdir(parents=True)
        data = candidate / "session"
        data.write_text("retained")
        (library / "Caches").write_text("not a directory")

        session = self.start()
        session.search(bundle_id)
        session.expect("Leftover candidates")
        session.expect("Uncheckable locations: 1")
        session.expect("Space to select")
        session.expect("Selected: 0")
        session.send(b" ")
        session.expect("Selected: 1")
        session.send(b"\r")
        session.expect("Cleanup preview")
        session.expect(re.escape(str(candidate)))
        session.expect("Includes all descendants")
        session.expect("may contain retained user data")
        session.expect(r"\[ Cancel \]")
        self.assertNotIn("Move to Trash", session.screen.text())
        self.assertEqual(data.read_text(), "retained")
        session.send(b"\r")
        session.expect("Leftover candidates")
        session.expect("Selected: 1")
        session.quit()
        self.assertEqual(data.read_text(), "retained")

    def test_preview_lists_only_the_parent_when_parent_and_child_are_selected(self):
        bundle_id = "org.lfd.tree"
        self.register_absent_bundle(bundle_id)
        directory = self.home / "Library" / "Application Support" / bundle_id
        directory.mkdir(parents=True)
        child = directory / "session"
        child.write_text("retained")
        session = self.start()
        session.search(bundle_id)
        session.expect(re.escape(str(directory)))
        session.expect("Selected: 0")
        session.send(b"\x1b[B")
        session.expect(re.escape(str(child)))
        session.send(b" \r")
        session.expect("Cleanup preview")
        session.expect(re.escape(str(child)))
        session.send(b"\x1b")
        session.expect("Leftover candidates")
        session.expect("Selected: 1")
        session.send(b"\x1b[A \r")
        session.expect("Cleanup preview")
        session.expect(re.escape(str(directory)))
        self.assertNotIn(str(child), session.screen.text())
        session.expect("Includes all descendants")
        session.send(b"\r")
        session.expect("Selected: 2")
        session.send(b" ")
        session.expect("Selected: 1")
        session.quit()
        self.assertEqual(child.read_text(), "retained")

    def test_preview_distinguishes_literal_backslashes_from_escaped_newlines(self):
        bundle_id = "org.lfd.escaped"
        self.register_absent_bundle(bundle_id)
        directory = self.home / "Library" / "Application Support" / bundle_id
        directory.mkdir(parents=True)
        newline = directory / "session\nbackup"
        backslash = directory / r"session\nbackup"
        newline.write_text("newline data")
        backslash.write_text("backslash data")
        session = self.start()
        session.search(bundle_id)
        session.expect("Selected: 0")
        session.send(b"\x1b[B \x1b[B \r")
        session.expect("Cleanup preview")
        session.expect(re.escape(str(backslash)) + r" +│")
        session.expect(re.escape(str(directory / r"session\\nbackup")) + r" +│")
        session.send(b"\r")
        session.expect("Selected: 2")
        session.quit()
        self.assertEqual(newline.read_text(), "newline data")
        self.assertEqual(backslash.read_text(), "backslash data")

    def test_incomplete_required_checks_keep_verified_data_view_only(self):
        bundle_id = "org.lfd.partial"
        records = self.register_absent_bundle(bundle_id)
        candidate = self.home / "Library" / "Application Support" / bundle_id
        candidate.mkdir(parents=True)
        for missing_check in ("application directory", "supplemental response"):
            with self.subTest(missing_check=missing_check):
                if missing_check == "application directory":
                    (self.home / "Applications").write_text("not a directory")
                else:
                    (self.home / "Applications").unlink()
                    (records / f"{bundle_id}.checks").write_text("PATH: not found\n")
                session = self.start()
                session.search(bundle_id)
                session.expect(re.escape(str(candidate)))
                session.expect("Standard app-specific data path matches the registered Bundle ID")
                session.expect("View only: required main program checks incomplete")
                session.expect("Selection disabled: required main program checks incomplete")
                session.send(b" \r")
                session.expect("Nothing selected for preview")
                self.assertNotIn("Cleanup preview", session.screen.text())
                session.quit()
        self.assertTrue(candidate.is_dir())

    def test_links_and_lookalikes_outside_the_cleanup_scope_stay_view_only(self):
        bundle_id = "org.lfd.scope"
        self.register_absent_bundle(bundle_id)
        outside = self.home / "Downloads" / bundle_id
        outside.mkdir(parents=True)
        target = outside / "session"
        target.write_text("outside data")
        link = self.home / "Library" / "Caches" / bundle_id
        link.parent.mkdir(parents=True)
        link.symlink_to(outside, target_is_directory=True)
        lookalike = self.home / "Library" / "Preferences" / bundle_id
        lookalike.parent.mkdir()
        lookalike.write_text("not a preference plist")
        session = self.start()
        session.search(bundle_id)
        session.expect(re.escape(str(lookalike)))
        session.expect("View only: outside the cleanup scope")
        session.send(b" \x1b[B")
        session.expect(re.escape(str(link)))
        session.expect("View only: symbolic links cannot be selected")
        self.assertNotIn(str(outside), session.screen.text())
        session.send(b" \r")
        session.expect("Selected: 0 · Nothing selected for preview")
        session.quit()
        self.assertEqual(lookalike.read_text(), "not a preference plist")
        self.assertEqual(link.readlink(), outside)
        self.assertEqual(target.read_text(), "outside data")

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

    def test_searches_named_entries_in_the_additional_user_data_locations(self):
        candidates = (
            "Library/Containers/Scope Tool",
            "Library/Group Containers/Scope Tool",
            "Library/Logs/Scope Tool",
            "Library/Saved Application State/Scope Tool.savedState",
            ".config/Scope Tool",
            ".local/share/Scope Tool",
            ".local/state/Scope Tool",
            ".cache/Scope Tool",
        )
        for relative in candidates:
            candidate = self.home / relative
            candidate.parent.mkdir(parents=True, exist_ok=True)
            candidate.write_text("isolated data")
        session = self.start()
        session.search("Scope Tool")
        for index, relative in enumerate(candidates):
            with self.subTest(location=relative):
                if index:
                    session.send(b"\x1b[B")
                session.expect(re.escape(str(self.home / relative)))
                session.expect("Filename match only; application ownership not established")
        session.quit()
        for relative in candidates:
            self.assertEqual((self.home / relative).read_text(), "isolated data")

    def test_searches_named_entries_in_the_isolated_private_temporary_directory(self):
        candidate = self.root / "temp" / "org.lfd.temporary"
        candidate.parent.mkdir()
        candidate.write_text("isolated temporary data")
        session = self.start()
        session.search(candidate.name)
        session.expect(re.escape(str(candidate)))
        session.expect("Type: Temporary data")
        session.expect("Filename match only; application ownership not established")
        session.quit()
        self.assertEqual(candidate.read_text(), "isolated temporary data")

    def test_reports_references_in_only_the_listed_opencode_configs_without_editing_them(self):
        sources = {
            ".config/opencode/cli.json": '{"plugin":["scope-tool"],"other":"EXAMPLE-SETTING"}',
            ".config/opencode/tui.jsonc": '{"plugin":["Scope-Tool"]}',
            ".config/opencode/other.json": '{"plugin":["scope-tool"]}',
            ".config/another-tool/cli.json": '{"plugin":["scope-tool"]}',
            ".config/nested/scope-tool": "out of scope filename",
            "Library/Application Support/Browser/scope-tool": "out of scope filename",
            ".zsh_history": "scope-tool",
        }
        for relative, content in sources.items():
            path = self.home / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        session = self.start()
        session.search("scope-tool")
        session.expect(re.escape(str(self.home / ".config/opencode/cli.json")))
        session.expect("Shared reference: matched scope-tool in file contents")
        session.expect("View only: a reference does not establish ownership")
        self.assertNotIn("EXAMPLE-SETTING", session.screen.text())
        self.assertNotIn("Filename match", session.screen.text())
        for unexpected in ("other.json", "another-tool", ".zsh_history"):
            self.assertNotIn(unexpected, session.screen.text())
        self.assertNotRegex(session.screen.text(), r"scope-tool +│")
        self.assertEqual(len(re.findall(r"(?m)^│[ >]*cli\.json +│", session.screen.text())), 1)
        session.send(b"\x1b[B")
        session.expect(re.escape(str(self.home / ".config/opencode/tui.jsonc")))
        session.quit()
        for relative, content in sources.items():
            self.assertEqual((self.home / relative).read_text(), content)

    def test_shared_references_and_group_container_stay_view_only_with_a_registered_id(self):
        bundle_id = "org.lfd.shared"
        self.register_absent_bundle(bundle_id)
        container = self.home / "Library" / "Group Containers" / bundle_id
        container.mkdir(parents=True)
        shared = self.home / ".config" / "opencode" / "cli.json"
        shared.parent.mkdir(parents=True)
        contents = '{"plugin":["org.lfd.shared"],"unrelated":true}'
        shared.write_text(contents)
        session = self.start()
        session.search(bundle_id)
        session.expect(re.escape(str(container)))
        session.expect("View only: outside the cleanup scope")
        session.send(b" \x1b[B")
        session.expect(re.escape(str(shared)))
        session.expect("View only: a reference does not establish ownership")
        session.send(b" \r")
        session.expect("Selected: 0 · Nothing selected for preview")
        session.quit()
        self.assertTrue(container.is_dir())
        self.assertEqual(shared.read_text(), contents)

    def test_reports_references_in_direct_pi_sources_without_following_links(self):
        extensions = self.home / ".pi/agent/extensions"
        outside = self.root / "outside.js"
        content = 'import "org.lfd.reference";'
        outside.write_text(content)
        sources = {
            "entry.js": content,
            "integration.ts": content,
            "manifest.json": content,
            "nested/index.ts": content,
            "tools.ts/index.ts": content,
            "other-id.ts": 'import "org.lfd.reference.extra";',
            "oversize.js": content + " " * (1024 * 1024),
        }
        for relative, text in sources.items():
            path = extensions / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        (extensions / "linked.ts").symlink_to(outside)
        opencode = self.home / ".config/opencode"
        target = self.root / "outside-config"
        target.mkdir()
        (target / "cli.json").write_text(content)
        opencode.parent.mkdir()
        opencode.symlink_to(target, target_is_directory=True)
        session = self.start()
        session.search("org.lfd.reference")
        session.expect(re.escape(str(extensions / "entry.js")))
        session.expect("Type: Extension source")
        session.expect(r"Shared reference: matched org\.lfd\.reference in file contents")
        session.expect("View only: a reference does not establish ownership")
        session.expect("Uncheckable locations: 4")
        for name in ("manifest.json", "index.ts", "other-id.ts", "oversize.js", "linked.ts", "cli.json"):
            self.assertNotIn(name, session.screen.text())
        session.send(b"\x1b[B")
        session.expect(re.escape(str(extensions / "integration.ts")))
        session.send(b"i")
        session.expect(re.escape(str(opencode / "cli.json")))
        session.expect("symbolic link not followed")
        session.send(b"\x1b[B\x1b[B")
        session.expect(re.escape(str(extensions / "linked.ts")))
        session.expect("symbolic link not followed")
        session.send(b"\x1b[B")
        session.expect(re.escape(str(extensions / "oversize.js")))
        session.expect("reference source exceeds 1 MiB")
        session.quit()
        self.assertTrue((extensions / "linked.ts").is_symlink())
        self.assertEqual(outside.read_text(), content)
        self.assertEqual((target / "cli.json").read_text(), content)
        for relative, text in sources.items():
            self.assertEqual((extensions / relative).read_text(), text)

    def test_exact_bundle_id_uses_local_metadata_and_labels_related_items(self):
        apps = self.root / "System" / "Applications" / "Utilities"
        bundle = make_app(apps, "Internal.app", "Named Tool", "org.lfd.named", binary_plist=True)
        make_app(apps, "Other.app", "Other Tool", "org.lfd.named.extra")
        # Negative supplemental results must not override an app found on disk.
        self.register_absent_bundle("org.lfd.named")
        preference = self.home / "Library" / "Preferences" / "org.lfd.named.plist"
        preference.parent.mkdir(parents=True)
        preference.write_text("isolated preferences")
        session = self.start()
        session.search("org.lfd.named")
        session.expect("Main program found")
        session.expect("Related items")
        session.expect(re.escape(str(bundle)))
        session.expect(r"CFBundleIdentifier: org\.lfd\.named(?![.\w-])")
        session.expect("View only: outside the cleanup scope")
        self.assertNotIn("Other.app", session.screen.text())
        session.send(b"\x1b[B")
        session.expect(re.escape(str(preference)))
        session.expect("Preferences")
        session.expect("Risk: may contain retained settings")
        session.expect("View only: main program found")
        session.send(b" \r")
        session.expect("Selected: 0 · Nothing selected for preview")
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

    def test_query_and_standard_path_alone_cannot_be_selected(self):
        bundle_id = "org.lfd.unverified"
        records = self.register_absent_bundle(bundle_id)
        (records / f"{bundle_id}.plist").write_bytes(plistlib.dumps({
            "CFBundleIdentifier": "org.lfd.unrelated",
        }))
        candidate = self.home / "Library" / "Application Support" / bundle_id
        candidate.mkdir(parents=True)
        data = candidate / "session"
        data.write_text("retained")
        session = self.start()
        session.search(bundle_id)
        session.expect(re.escape(str(candidate)))
        session.expect("View only: application ownership not established")
        session.send(b" \r")
        session.expect("Selected: 0 · Nothing selected for preview")
        self.assertNotIn("Cleanup preview", session.screen.text())
        session.quit()
        self.assertEqual(data.read_text(), "retained")

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

    def test_exact_pkg_identifier_shows_registered_receipt_as_view_only(self):
        receipt = make_receipt(self.root, "org.lfd.receipt", "Receipt Tool")
        original = receipt.read_bytes()
        session = self.start()
        session.search("org.lfd.receipt")
        session.expect("Main program not found in the checked locations")
        session.expect(re.escape(str(receipt)))
        session.expect("Registered .pkg receipt")
        session.expect(r"Package ID: org\.lfd\.receipt")
        session.expect("View only")
        session.expect("historical evidence")
        session.quit()
        self.assertEqual(receipt.read_bytes(), original)

    def test_receipt_matches_stay_view_only_in_cleanup_preview(self):
        bundle_id = "org.lfd.receiptpreview"
        self.register_absent_bundle(bundle_id)
        caches = self.home / "Library" / "Caches"
        candidate = caches / bundle_id
        candidate.mkdir(parents=True)
        data = candidate / "data"
        data.write_text("retained")
        receipt = make_receipt(self.root, "org.lfd.installer", bundle_id, "/home/Library/Caches")
        bom = make_receipt_bom(receipt, [bundle_id])
        originals = {path: path.read_bytes() for path in (receipt, bom, data)}

        session = self.start()
        session.search(bundle_id)
        session.expect("Leftover candidates")
        session.expect(re.escape(str(receipt)))
        session.expect("View only: a receipt does not establish current ownership or installation")
        session.send(b" \x1b[B")
        session.expect(re.escape(str(candidate)))
        session.expect("Recorded by the registered .pkg receipt")
        session.expect("View only: a receipt does not establish current ownership or installation")
        session.send(b" \x1b[B \r")
        session.expect("Selected: 0 · Nothing selected for preview")
        session.quit()
        for path, original in originals.items():
            self.assertEqual(path.read_bytes(), original)

    def test_pkg_receipt_shows_only_existing_paths_in_the_fixed_scope(self):
        receipt = make_receipt(self.root, "org.lfd.paths", "Path Tool", "home/Library")
        caches = self.home / "Library" / "Caches"
        caches.mkdir(parents=True)
        current = caches / "Remembered Data"
        current.mkdir()
        missing = caches / "Missing Data"
        nested = current / "Hidden Data"
        nested.write_text("nested")
        outside = self.home / "Library" / "Unlisted Data"
        outside.write_text("outside")
        log = self.home / "Library" / "Logs" / "Recorded Log"
        log.parent.mkdir()
        log.write_text("retained log")
        bom = make_receipt_bom(receipt, [
            "Caches", "Caches/Remembered Data", "Caches/Missing Data", "Caches/Remembered Data/Hidden Data",
            "Unlisted Data", "Logs", "Logs/Recorded Log",
        ])
        original_bom = bom.read_bytes()
        session = self.start()
        session.search("org.lfd.paths")
        session.expect("Registered .pkg receipt")
        session.send(b"\x1b[B")
        session.expect(re.escape(str(current)))
        session.expect("Recorded by the registered .pkg receipt")
        session.expect("View only")
        for path in (missing, nested, outside):
            self.assertNotIn(path.name, session.screen.text())
        session.send(b"\x1b[B")
        session.expect(re.escape(str(log)))
        session.expect("Recorded by the registered .pkg receipt")
        session.quit()
        self.assertTrue(current.is_dir())
        self.assertEqual(outside.read_text(), "outside")
        self.assertEqual(nested.read_text(), "nested")
        self.assertEqual(log.read_text(), "retained log")
        self.assertFalse(missing.exists())
        self.assertEqual(bom.read_bytes(), original_bom)

    def test_chooses_one_same_named_pkg_receipt_without_merging_paths(self):
        first = make_receipt(self.root, "org.lfd.north", "Twin Installer", "/home/Library/Caches")
        second = make_receipt(self.root, "org.lfd.south", "Twin Installer", "/home/Library/Caches")
        caches = self.home / "Library" / "Caches"
        caches.mkdir(parents=True)
        for name in ("North Data", "South Data", "Twin Installer"):
            (caches / name).write_text(name)
        make_receipt_bom(first, ["North Data"])
        make_receipt_bom(second, ["South Data"])
        session = self.start()
        session.search("twin installer")
        session.expect("Choose one application")
        for text in ("org.lfd.north", "org.lfd.south", str(first), str(second)):
            session.expect(re.escape(text))
        session.send(b"\x1b[B\r")
        session.expect("Registered .pkg receipt")
        session.expect(re.escape(str(second)))
        session.expect("South Data")
        self.assertNotIn("North Data", session.screen.text())
        self.assertNotIn("org.lfd.north", session.screen.text())
        self.assertNotRegex(session.screen.text(), r"│ {1,3}Twin Installer +│")
        session.quit()

    def test_pkg_receipt_does_not_claim_a_different_app_at_its_old_path(self):
        receipt = make_receipt(self.root, "org.lfd.oldpkg", "Reused", "/Applications")
        make_receipt_bom(receipt, ["Reused.app"])
        bundle = make_app(self.root / "Applications", "Reused.app", "Reused", "org.lfd.newapp")
        data = self.home / "Library" / "Caches" / "org.lfd.newapp"
        data.mkdir(parents=True)
        session = self.start()
        session.search("org.lfd.oldpkg")
        session.expect("Main program check incomplete")
        session.expect("Possible matches")
        session.expect("Registered .pkg receipt")
        session.expect("View only")
        self.assertNotIn(bundle.name, session.screen.text())
        self.assertNotIn("org.lfd.newapp", session.screen.text())
        session.send(b"i")
        session.expect(re.escape(str(bundle)))
        session.expect("current Bundle ID org.lfd.newapp could not be tied to the selected receipt")
        session.quit()
        self.assertTrue(data.is_dir())

    def test_pkg_receipt_marks_a_replaced_app_path_uncheckable(self):
        receipt = make_receipt(self.root, "org.lfd.replaced", "Replacement Tool", "/Applications")
        make_receipt_bom(receipt, ["Replaced.app"])
        path = self.root / "Applications" / "Replaced.app"
        path.parent.mkdir()
        path.write_text("not an app bundle")
        session = self.start()
        session.search("org.lfd.replaced")
        session.expect("Main program check incomplete")
        session.expect("Registered .pkg receipt")
        session.expect("View only")
        self.assertNotIn(path.name, session.screen.text())
        session.send(b"i")
        session.expect(re.escape(str(path)))
        session.expect("recorded .app path is not an application directory")
        session.quit()
        self.assertEqual(path.read_text(), "not an app bundle")

    def test_pkg_receipt_corroborates_a_live_app_with_a_different_bundle_id(self):
        receipt = make_receipt(self.root, "org.lfd.installer", "Corroborated", "/Applications")
        applications = self.root / "Applications"
        bundle = make_app(applications, "Corroborated.app", "Corroborated", "org.lfd.liveapp")
        metadata = bundle / "Contents" / "Info.plist"
        original = metadata.read_bytes()
        subprocess.run([
            "/usr/bin/mkbom", str(applications), str(receipt.with_suffix(".bom")),
        ], check=True)
        session = self.start()
        session.search("org.lfd.installer")
        session.expect("Main program found in the checked locations")
        session.expect("Related items")
        session.expect("Registered .pkg receipt")
        session.send(b"\x1b[B")
        session.expect(re.escape(str(bundle)))
        session.expect("Verified current Bundle ID: org.lfd.liveapp")
        session.expect("View only")
        session.quit()
        self.assertEqual(metadata.read_bytes(), original)

    def test_pkg_id_requires_exact_case_and_falls_back_without_a_receipt_match(self):
        receipt = make_receipt(self.root, "org.lfd.case", "org.lfd.case")
        unrelated = self.home / "Downloads" / "Org.Lfd.Case.pkg"
        unrelated.parent.mkdir()
        unrelated.write_bytes(receipt.read_bytes())
        cache = self.home / "Library" / "Caches" / "Org.Lfd.Case"
        cache.parent.mkdir(parents=True)
        cache.write_text("retained")
        session = self.start()
        session.search("Org.Lfd.Case")
        session.expect("Leftover candidates")
        session.expect(re.escape(str(cache)))
        session.expect("Filename match only")
        self.assertNotIn("Registered .pkg receipt", session.screen.text())
        session.quit()
        self.assertEqual(cache.read_text(), "retained")
        self.assertEqual(unrelated.read_bytes(), receipt.read_bytes())


class CommandTests(IsolatedTests):
    def test_finds_exact_executable_on_path_without_launching_it(self):
        marker = self.root / "unexpected-launch"
        bins = self.home / "bin"
        bins.mkdir()
        command = bins / "downloaded-tool"
        script = f"#!/bin/sh\n: > {shlex.quote(str(marker))}\n"
        command.write_text(script)
        command.chmod(0o755)
        decoys = self.home / "non-executable"
        decoys.mkdir()
        (decoys / command.name).write_text("not executable")
        session = self.start(decoys, bins)
        session.search(command.name)
        session.expect("Related items")
        session.expect(re.escape(str(command)))
        session.expect("Executable name matches the query in PATH")
        session.quit()
        self.assertEqual(command.read_text(), script)
        self.assertFalse(marker.exists())

    def test_path_command_keeps_registered_bundle_data_view_only(self):
        bundle_id = "org.lfd.executable"
        self.register_absent_bundle(bundle_id)
        command = self.home / "bin" / bundle_id
        command.parent.mkdir()
        command.write_text("synthetic executable")
        command.chmod(0o755)
        cache = self.home / "Library" / "Caches" / bundle_id
        cache.mkdir(parents=True)
        session = self.start(command.parent)
        session.search(bundle_id)
        session.expect("Main program found in the checked locations")
        session.expect(re.escape(str(command)))
        session.expect("View only: outside the cleanup scope")
        session.send(b" \x1b[B")
        session.expect(re.escape(str(cache)))
        session.expect("Standard app-specific data path matches the registered Bundle ID")
        session.expect("View only: main program found")
        session.send(b" \r")
        session.expect("Selected: 0 · Nothing selected for preview")
        session.quit()
        self.assertEqual(command.read_text(), "synthetic executable")
        self.assertTrue(cache.is_dir())

    def test_checks_formula_cellar_using_a_local_installation_record(self):
        keg = self.root / "opt/homebrew/Cellar/formula-tool@2/2.0"
        binary = keg / "bin/different-command"
        binary.parent.mkdir(parents=True)
        binary.write_text("synthetic executable")
        binary.chmod(0o755)
        receipt = keg / "INSTALL_RECEIPT.json"
        receipt.write_text('{"homebrew_version":"6.0.0","source":{"tap":"homebrew/core"}}')
        session = self.start()
        session.search("formula-tool@2")
        session.expect("Related items")
        session.expect(re.escape(str(binary)))
        session.expect("Local Homebrew installation record")
        session.expect("Homebrew global location: view-only")
        session.send(b"\x1b[B")
        session.expect(re.escape(str(receipt)))
        session.expect("Homebrew formula installation record")
        session.quit()
        self.assertEqual(binary.read_text(), "synthetic executable")
        self.assertTrue(receipt.is_file())

    def test_checks_cask_app_using_local_record_without_following_staged_link(self):
        cask = self.root / "usr/local/Caskroom/cask-new-name"
        receipt = cask / ".metadata/INSTALL_RECEIPT.json"
        receipt.parent.mkdir(parents=True)
        receipt.write_text(json.dumps({"source": {"version": "2.0"}, "uninstall_artifacts": [
            {"app": ["Original.app", {"target": "Graphical Choice.app"}]},
            {"zap": [{"trash": ["~/donotread"]}]},
        ]}))
        bundle = make_app(self.root / "Applications", "Graphical Choice.app", "Graphical Choice", "org.lfd.cask")
        external = make_app(self.root / "outside", "Unrelated.app", "Unrelated", "org.lfd.unrelated")
        staged = cask / "2.0/Original.app"
        staged.parent.mkdir()
        staged.symlink_to(external, target_is_directory=True)
        session = self.start()
        session.search(cask.name)
        session.expect("Related items")
        session.expect(re.escape(str(bundle)))
        session.expect("CFBundleIdentifier: org.lfd.cask")
        session.expect("Local Homebrew installation record")
        self.assertNotIn("org.lfd.unrelated", session.screen.text())
        self.assertNotIn("Uncheckable locations", session.screen.text())
        session.send(b"\x1b[B")
        session.expect(re.escape(str(receipt)))
        session.expect("Homebrew cask installation record")
        session.quit()
        self.assertTrue(staged.is_symlink())
        self.assertTrue(bundle.is_dir())

    def test_checks_cask_binary_payload_and_ignores_paths_outside_the_package(self):
        cask = self.root / "opt/homebrew/Caskroom/command-cask"
        binary = cask / "2.0/toolkit/cask-command"
        binary.parent.mkdir(parents=True)
        binary.write_text("cask executable")
        binary.chmod(0o755)
        outside = self.root / "outside-executable"
        outside.write_text("not installed by the cask")
        outside.chmod(0o755)
        receipt = cask / ".metadata/INSTALL_RECEIPT.json"
        receipt.parent.mkdir()
        receipt.write_text(json.dumps({"source": {"version": "2.0"}, "uninstall_artifacts": [
            {"binary": [str(binary)]}, {"binary": [str(outside)]},
        ]}))
        session = self.start()
        session.search(cask.name)
        session.expect("Main program found")
        session.expect(re.escape(str(binary)))
        session.expect("Local Homebrew installation record")
        session.send(b"\x1b[B")
        session.expect(re.escape(str(receipt)))
        self.assertNotIn(outside.name, session.screen.text())
        session.quit()
        self.assertEqual(outside.read_text(), "not installed by the cask")

    def test_keeps_name_evidence_when_formula_installation_record_is_missing(self):
        keg = self.root / "usr/local/Cellar/unrecorded-tool/2.0"
        command = keg / "bin/unrecorded-tool"
        command.parent.mkdir(parents=True)
        command.write_text("unrecorded executable")
        command.chmod(0o755)
        other = command.with_name("unrelated-command")
        other.write_text("no exact name match")
        other.chmod(0o755)
        cache = self.home / "Library/Caches/unrecorded-tool"
        cache.mkdir(parents=True)
        session = self.start()
        session.search(command.name)
        session.expect("Related items")
        session.expect(re.escape(str(command)))
        session.expect("No local Homebrew installation record")
        session.expect("application ownership not established")
        self.assertNotIn(other.name, session.screen.text())
        session.send(b"\x1b[B")
        session.expect(re.escape(str(cache)))
        session.expect("Filename match only; application ownership not established")
        session.quit()
        self.assertFalse((keg / "INSTALL_RECEIPT.json").exists())
        self.assertTrue(cache.is_dir())

    def test_record_without_a_program_is_a_leftover_candidate(self):
        cask = self.root / "opt/homebrew/Caskroom/former-app"
        receipt = cask / ".metadata/INSTALL_RECEIPT.json"
        receipt.parent.mkdir(parents=True)
        receipt.write_text(json.dumps({"source": {"version": "1.0"}, "uninstall_artifacts": [
            {"app": ["Former App.app"]},
        ]}))
        session = self.start()
        session.search(cask.name)
        session.expect("Main program not found in the checked locations")
        session.expect("Leftover candidates")
        session.expect(re.escape(str(receipt)))
        session.expect("not proof of a current main program")
        session.quit()
        self.assertTrue(receipt.is_file())

    def test_unknown_program_reports_a_blocked_path_check_without_following_it(self):
        target = self.root / "not-on-path"
        target.mkdir()
        binary = target / "unknown-tool"
        binary.write_text("not reached through PATH links")
        binary.chmod(0o755)
        linked = self.home / "linked-bin"
        linked.symlink_to(target, target_is_directory=True)
        session = self.start(linked)
        session.search(binary.name)
        session.expect("Main program check incomplete")
        session.expect("No matching items")
        session.send(b"i")
        session.expect("PATH command check")
        session.expect("symbolic link not followed")
        session.quit()
        self.assertTrue(linked.is_symlink())
        self.assertEqual(binary.read_text(), "not reached through PATH links")

    def test_invalid_formula_record_is_reported_without_claiming_the_keg_binary(self):
        package = self.root / "opt/homebrew/Cellar/unverified-formula"
        unrelated = package / "1.0/bin/unrelated-tool"
        unrelated.parent.mkdir(parents=True)
        unrelated.write_text("no evidence connecting this executable")
        unrelated.chmod(0o755)
        receipt = package / "1.0/INSTALL_RECEIPT.json"
        receipt.write_text("not a valid Homebrew record")
        cache = self.home / "Library/Caches" / package.name
        cache.mkdir(parents=True)
        session = self.start()
        session.search(package.name)
        session.expect("Main program check incomplete")
        session.expect(re.escape(str(cache)))
        session.expect("application ownership not established")
        self.assertNotIn(unrelated.name, session.screen.text())
        session.send(b"i")
        session.expect(re.escape(str(receipt)))
        session.expect("Homebrew location or record")
        session.quit()
        self.assertEqual(receipt.read_text(), "not a valid Homebrew record")

    def test_chooses_one_identity_across_same_named_app_command_and_cask(self):
        name = "shared-tool"
        existing = make_app(self.root / "Applications", "Unrelated.app", name, "org.lfd.separate")
        command = self.home / "bin" / name
        command.parent.mkdir()
        command.write_text("standalone command")
        command.chmod(0o755)
        cask = self.root / "opt/homebrew/Caskroom" / name
        receipt = cask / ".metadata/INSTALL_RECEIPT.json"
        receipt.parent.mkdir(parents=True)
        bundled = make_app(self.root / "Applications", "Cask Bundle.app", "Cask Bundle", "org.lfd.caskbundle")
        receipt.write_text(json.dumps({"source": {"version": "1.0"}, "uninstall_artifacts": [
            {"app": [bundled.name]},
        ]}))
        for cache in (name, "org.lfd.separate", "org.lfd.caskbundle"):
            (self.home / "Library/Caches" / cache).mkdir(parents=True)
        session = self.start(command.parent)
        session.search(name)
        session.expect("Choose one application")
        for detail in (str(existing), str(command), "Homebrew cask: shared-tool"):
            session.expect(re.escape(detail))
        session.send(b"\x1b[B\x1b[B")
        session.expect("Bundle ID: org.lfd.caskbundle")
        session.expect(re.escape(str(bundled)))
        session.send(b"\r")
        session.expect("Related items")
        session.expect(re.escape(str(bundled)))
        self.assertNotIn(existing.name, session.screen.text())
        self.assertNotIn("org.lfd.separate", session.screen.text())
        self.assertNotRegex(session.screen.text(), r"│ {1,3}shared-tool +│")
        session.quit()


class DefaultBuildTests(IsolatedTests):
    def test_rejects_arguments_instead_of_searching_an_arbitrary_path(self):
        result = subprocess.run(
            [BINARY, "/Applications/Example.app"], capture_output=True, text=True, timeout=5,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("lfd does not take arguments", result.stderr)

    def test_searches_the_native_macos_private_temporary_directory(self):
        directory = subprocess.check_output(
            ["/usr/bin/getconf", "DARWIN_USER_TEMP_DIR"], text=True,
        ).strip()
        candidate = tempfile.TemporaryDirectory(prefix="lfd-", dir=directory)
        self.addCleanup(candidate.cleanup)
        path = Path(candidate.name).resolve()
        session = self.start()
        session.search(path.name)
        session.expect(re.escape(str(path)))
        session.expect("Type: Temporary data")
        session.quit()
        self.assertTrue(path.is_dir())

    def test_ignores_fixture_override(self):
        bundle_id = "org.lfd.fixtureonly7938A51"
        self.register_absent_bundle(bundle_id)
        make_app(self.root / "Applications", "Fixture Only.app", "Fixture Only", bundle_id)
        ordinary_home = self.root / "ordinary-home"
        cache = ordinary_home / "Library" / "Caches" / bundle_id
        cache.mkdir(parents=True)
        session = Session(BINARY, ordinary_home, self.root)
        self.addCleanup(session.close)
        session.search(bundle_id)
        session.expect(re.escape(str(cache)))
        session.expect("Filename match only")
        session.expect("Selection disabled: required main program checks incomplete")
        self.assertNotIn("Fixture Only.app", session.screen.text())
        session.send(b" \r")
        session.expect("Selected: 0 · Nothing selected for preview")
        session.quit()
        self.assertTrue(cache.is_dir())


if __name__ == "__main__":
    BINARY = os.path.abspath(sys.argv[1])
    tests = sys.argv[2:] or ["FixtureTests", "CommandTests"]
    unittest.main(argv=[sys.argv[0], *tests], verbosity=2)

"""Exercise rmem in a Unix PTY, using only the Python standard library."""

import codecs
import errno
import fcntl
import os
import pty
import re
import select
import shlex
import signal
import struct
import sys
import termios
import time
import unicodedata


ROWS, COLS = 40, 140
CSI = re.compile(r"\x1b\[([0-?]*)([ -/]*)([@-~])")


class Screen:
    """Handle the alternate screen and cursor-addressed updates Ratatui emits."""

    def __init__(self):
        self.decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")
        self.pending = ""
        self.active = False
        self.clear()

    def clear(self):
        self.cells = [[" "] * COLS for _ in range(ROWS)]
        self.row = self.col = 0

    def feed(self, data):
        stream = self.pending + self.decoder.decode(data)
        self.pending = ""
        i = 0
        while i < len(stream):
            ch = stream[i]
            if ch == "\x1b":
                if i + 1 == len(stream):
                    self.pending = stream[i:]
                    break
                if stream[i + 1] != "[":
                    i += 2
                    continue
                sequence = CSI.match(stream, i)
                if not sequence:
                    self.pending = stream[i:]
                    break
                params, _, command = sequence.groups()
                if params == "?1049" and command in "hl":
                    self.active = command == "h"
                    self.clear()
                elif self.active and command in "Hf":
                    row, _, col = params.partition(";")
                    self.row, self.col = int(row or 1) - 1, int(col or 1) - 1
                elif self.active and command == "J" and params == "2":
                    self.clear()
                i = sequence.end()
                continue
            if self.active and ch.isprintable() and not unicodedata.combining(ch):
                if 0 <= self.row < ROWS and 0 <= self.col < COLS:
                    self.cells[self.row][self.col] = ch
                self.col += 2 if unicodedata.east_asian_width(ch) in "WF" else 1
            i += 1

    def text(self):
        return "\n".join("".join(row).rstrip() for row in self.cells)


def smoke(binary):
    shell, fd = pty.fork()
    if shell == 0:
        os.environ.update(TERM="xterm-256color", PS1="rmem-smoke> ", ENV="")
        os.execl("/bin/sh", "sh", "-i")

    screen = Screen()

    def send(data):
        os.write(fd, data)

    def expect(pattern, label, *, terminal=False):
        deadline = time.monotonic() + 10
        raw = bytearray()
        while time.monotonic() < deadline:
            if not select.select([fd], [], [], max(0, deadline - time.monotonic()))[0]:
                break
            try:
                chunk = os.read(fd, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break
            if not chunk:
                break
            raw.extend(chunk)
            screen.feed(chunk)
            content = raw.decode(errors="replace") if terminal else screen.text()
            match = re.search(pattern, content)
            if match:
                return match
        raise AssertionError(f"{label}: {content[-4000:] if raw else '<no output>'}")

    try:
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        expect(r"rmem-smoke> ", "initial shell prompt", terminal=True)
        before = termios.tcgetattr(fd)
        send((shlex.quote(binary) + "; printf 'rmem-exit:%s\\n' \"$?\"\n").encode())
        expect(r"Processes \d+ / \d+[\s\S]*gg/G first/last", "process list shortcut hint")
        send(b"/")
        expect("SEARCH", "search mode")
        pid = str(shell)
        send(pid.encode() + b"\r")
        expect(r"filter: " + pid + r"\b[^\n]*gg/G first/last", "PID filter shortcut hint")

        # Search is substring-based, so more than one row can match a PID.
        for _ in range(30):
            send(b"\r")
            details = expect(r"· PID (\d+)\b[\s\S]*?Enter / Esc / q  close", "process details")
            send(b"\x1b")
            expect(r"\A(?![\s\S]*Enter / Esc / q  close)[\s\S]*filter: " + pid, "process list after Esc")
            if details[1] == pid:
                break
            send(b"j")
        else:
            raise AssertionError(f"Could not open details for PID {shell}")

        send(b"q")
        expect(r"rmem-exit:0\r?\n[\s\S]*?rmem-smoke> ", "successful exit after one q", terminal=True)
        assert termios.tcgetattr(fd) == before, "terminal settings were not restored"
        send(b"printf '%s%s\\n' rmem-shell- restored\n")
        expect(r"rmem-shell-restored\r?\n", "shell executes input after exit", terminal=True)
        print("rmem: shortcut hints, PID search, matching details, Esc, q, and terminal restoration passed")
    finally:
        os.close(fd)
        try:
            os.kill(shell, signal.SIGKILL)
        except ProcessLookupError:
            pass
        os.waitpid(shell, 0)


if __name__ == "__main__":
    smoke(os.path.abspath(sys.argv[1]))

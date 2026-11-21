"""Exercise a built rmem.exe in a local WinPTY using Windows Python."""

import argparse
from contextlib import ExitStack
import ctypes as c
from ctypes import wintypes as w
import os
from pathlib import Path
import re
import sys
import threading
import time


PROMPT = b"rmem-smoke>"
ESCAPES = re.compile(rb"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*\x07)")


def smoke(binary, dll):
    winpty = c.CDLL(str(dll))
    kernel = c.WinDLL("kernel32", use_last_error=True)
    handle, string = c.c_void_p, c.c_wchar_p
    error_out = c.POINTER(handle)
    error = handle()

    def api(name, result, *args):
        symbol = "winpty_" + name
        try:
            fn = getattr(winpty, symbol)
        except AttributeError as exc:
            raise RuntimeError(f"{dll} does not export {symbol}") from exc
        fn.argtypes, fn.restype = args, result
        return fn

    def checked(step, result):
        if not result:
            message = api("error_msg", string, handle)(error) if error else None
            if error:
                api("error_free", None, handle)(error)
            raise RuntimeError(f"{step}: {message or 'WinPTY returned no error details'}")
        return result

    config = checked("create WinPTY configuration", api(
        "config_new", handle, c.c_uint64, error_out
    )(0, c.byref(error)))
    try:
        api("config_set_initial_size", None, handle, c.c_int, c.c_int)(config, 145, 42)
        agent = checked("open WinPTY", api("open", handle, handle, error_out)(config, c.byref(error)))
    finally:
        api("config_free", None, handle)(config)

    with ExitStack() as pipes:
        try:
            stdin = pipes.enter_context(open(api("conin_name", string, handle)(agent), "wb", buffering=0))
            stdout = pipes.enter_context(open(api("conout_name", string, handle)(agent), "rb", buffering=0))
            chunks = []

            def reader():
                try:
                    while data := stdout.read(65536):
                        chunks.append(data)
                except OSError as exc:
                    read_errors.append(exc)
                finally:
                    ended.set()

            ended = threading.Event()
            read_errors = []
            threading.Thread(target=reader, daemon=True).start()

            def expect(pattern, label, start=0):
                deadline = time.monotonic() + 11
                while True:
                    data = b"".join(chunks)
                    plain = ESCAPES.sub(b"", data[start:])
                    if match := re.search(pattern, plain):
                        return len(data), match
                    if ended.is_set() or time.monotonic() >= deadline:
                        reason = (f"output pipe closed ({read_errors[0]})" if read_errors
                                  else "output pipe closed" if ended.is_set() else "timed out")
                        raise RuntimeError(f"{label}: {reason}; output tail: {plain[-500:]!r}")
                    ended.wait(0.05)

            # Expand the executable path inside CMD instead of encoding it as terminal input.
            env = dict(os.environ, PROMPT="rmem-smoke$G", RMEM_SMOKE_EXE=str(binary))
            env_block = "\0".join(f"{key}={value}" for key, value in sorted(env.items())) + "\0\0"
            spawn_config = checked("configure CMD", api(
                "spawn_config_new", handle, c.c_uint64, string, string, string, string, error_out
            )(0, None, "cmd.exe /d /q /v:off", None, env_block, c.byref(error)))
            child, win_error = w.HANDLE(), w.DWORD()
            try:
                checked("spawn CMD", api(
                    "spawn", w.BOOL, handle, handle, c.POINTER(w.HANDLE),
                    c.POINTER(w.HANDLE), c.POINTER(w.DWORD), error_out
                )(agent, spawn_config, c.byref(child), None, c.byref(win_error), c.byref(error)))
            finally:
                api("spawn_config_free", None, handle)(spawn_config)

            kernel.CloseHandle.argtypes = [w.HANDLE]
            kernel.CloseHandle.restype = w.BOOL
            kernel.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
            kernel.WaitForSingleObject.restype = w.DWORD
            try:
                send = stdin.write
                start, _ = expect(PROMPT, "initial CMD prompt")
                send(b'"%RMEM_SMOKE_EXE%"\r')
                start, _ = expect(rb"COMMAND", "rmem process list", start)
                pid = str(os.getpid()).encode("ascii")
                send(b"/" + pid + b"\r")
                start, _ = expect(rb"filter: " + pid + rb"\b", "PID filter", start)

                # The search can match other rows, so verify the PID in the details title.
                for _ in range(30):
                    send(b"\r")
                    _, details = expect(rb"\bPID (\d+)\b", "process details", start)
                    # Let WinPTY finish emitting the details redraw before checking the list.
                    time.sleep(0.2)
                    start = sum(map(len, chunks))
                    send(b"\x1b")
                    row = rb"(?<!\d)" + details[1] + rb" +\S+ +\d+\.\d"
                    start, _ = expect(row, "process list after Esc", start)
                    if details[1] == pid:
                        break
                    send(b"j")
                else:
                    raise RuntimeError(f"could not open details for PID {pid.decode('ascii')}")

                send(b"q")
                start, _ = expect(PROMPT, "CMD prompt after q", start)
                command = b"echo rmem-shell-restored:%errorlevel%"
                send(command)
                start, _ = expect(re.escape(command), "CMD echoes typed input", start)
                send(b"\r")
                expect(rb"rmem-shell-restored:0\r?\n", "CMD executes input and rmem exited with status 0", start)
                send(b"exit\r")
                if kernel.WaitForSingleObject(child, 3000) != 0:
                    raise RuntimeError("CMD did not exit after the exit command")
            finally:
                kernel.CloseHandle(child)
        finally:
            api("free", None, handle)(agent)


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog="Set WINPTY_DLL to the path of winpty.dll.")
    parser.add_argument("binary", type=Path, help="path to a built Windows rmem.exe")
    args = parser.parse_args()
    binary = args.binary.resolve()
    dll = os.environ.get("WINPTY_DLL")
    if sys.platform != "win32":
        blocked = "run with native Windows Python"
    elif not dll or not Path(dll).is_file():
        blocked = "set WINPTY_DLL to an existing winpty.dll"
    elif not binary.is_file():
        blocked = f"built rmem.exe not found: {binary}"
    else:
        try:
            smoke(binary, Path(dll).resolve())
        except (OSError, RuntimeError) as exc:
            print(f"FAIL: {exc}", file=sys.stderr)
            return 1
        print("PASS: PID search, matching details, Esc, q, and CMD input restoration")
        return 0
    print(f"BLOCKED: {blocked}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())

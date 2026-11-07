#!/usr/bin/env python3
import fcntl
import os
import select
import struct
import tempfile
import termios
import time
import unicodedata

binary = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "target",
    "release",
    "apkez",
)

def pump(fd, seconds=0.5):
    output = bytearray()
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        ready, _, _ = select.select([fd], [], [], max(0, end - time.monotonic()))
        if not ready:
            break
        try:
            output.extend(os.read(fd, 65536))
        except OSError:
            break
    return bytes(output)

def visible_text(data):
    text = data.decode("utf-8", errors="replace")
    result = []
    index = 0
    while index < len(text):
        char = text[index]
        if char == "\x1b":
            if index + 1 < len(text) and text[index + 1] == "[":
                index += 2
                while index < len(text) and not ("@" <= text[index] <= "~"):
                    index += 1
                index += 1
                continue
            index += 1
            continue
        if unicodedata.east_asian_width(char) in ("W", "F"):
            result.append("  ")
        else:
            result.append(char)
        index += 1
    return "".join(result)

with tempfile.TemporaryDirectory(prefix="apkez-interactive-") as root:
    pid, fd = os.forkpty()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.environ["SHELL"] = "/bin/bash"
        os.environ["PS1"] = "$ "
        os.chdir(root)
        os.execv(binary, [binary, "--root", root])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    output = bytearray()
    exited = False

    def send(data, wait=0.3):
        os.write(fd, data.encode())
        output.extend(pump(fd, wait))

    try:
        output.extend(pump(fd, 1))
        send("\x1b[17~")
        send("printf 'terminal-ready\\n'\r", 1)
        text = visible_text(output)
        assert "terminal-ready" in text, text[-3000:]

        send("vim -u NONE -i NONE -N check.txt\r", 1)
        send("iapkez-pty-success", 0.4)
        send("\x1b", 0.3)
        send(":wq\r", 1)
        with open(os.path.join(root, "check.txt"), encoding="utf-8") as saved:
            assert saved.read().strip() == "apkez-pty-success"

        send("cd /tmp\r", 0.8)
        send("\x1d", 0.5)
        send("\x1b[17~", 0.8)
        text = visible_text(output)
        assert "/tmp" in text, "interactive shell directory was lost"

        send("\x11")
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            waited, status = os.waitpid(pid, os.WNOHANG)
            if waited == pid:
                exited = True
                assert os.waitstatus_to_exitcode(status) == 0, status
                break
            output.extend(pump(fd, 0.1))
        assert exited, "application did not exit after Ctrl+Q"
        print("PASS: interactive shell, vim editing/saving, persistent cwd, clean exit")
    finally:
        if not exited:
            os.kill(pid, 9)
            os.waitpid(pid, 0)
        os.close(fd)

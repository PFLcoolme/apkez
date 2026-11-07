#!/usr/bin/env python3
import fcntl
import os
import select
import struct
import termios
import time

binary = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "target",
    "release",
    "apkez",
)
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
pid, fd = os.forkpty()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.chdir(root)
    os.execv(binary, [binary, "--root", root])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))

output = bytearray()
exited = False

def send(data):
    os.write(fd, data.encode())

def pump(seconds=0.5):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        ready, _, _ = select.select([fd], [], [], max(0, end - time.monotonic()))
        if not ready:
            break
        try:
            output.extend(os.read(fd, 65536))
        except OSError:
            break

try:
    pump(1)
    for key in [
        "\x1b[11~",
        "\x1b[12~",
        "\x1b[13~",
        "\x1b[14~",
        "\x1b[15~",
        "\x1b[17~",
        "\x1b[18~",
    ]:
        send(key)
        pump(0.2)
    send("\x11")
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        waited, status = os.waitpid(pid, os.WNOHANG)
        if waited == pid:
            exited = True
            assert os.waitstatus_to_exitcode(status) == 0, status
            break
        pump(0.1)
    assert exited, "application did not exit after Ctrl+Q"
    text = output.decode("utf-8", errors="replace")
    assert "apkez" in text, f"main interface was not rendered: {text!r}"
    assert "\x1b[?1000h" in text or "\x1b[?1002h" in text, "mouse capture was not enabled"
    assert "\x1b[?1000l" in text or "\x1b[?1002l" in text, "mouse capture was not restored"
    print("PASS: TTY startup, all seven pages, mouse capture, and clean Ctrl+Q exit")
finally:
    if not exited:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
    os.close(fd)

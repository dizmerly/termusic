#!/usr/bin/env python3
"""Drive termusic in a pty and save screenshots (.txt and .png) to docs/screenshots/.

Needs `pip install pyte pillow`. Usage:
  scripts/capture.py w:2 t:daft\\ punk k:enter w:8 snap:item2-results k:j snap:x q
Steps: w:<secs> wait, t:<text> type, k:<enter|esc|down|up|space|tab> key, snap:<name> capture.
Always ends by sending Ctrl-C and reporting leftover mpv processes.
"""
import fcntl, os, pty, select, struct, subprocess, sys, termios, time
import pyte
from PIL import Image, ImageDraw, ImageFont

COLS, ROWS = 100, 30
KEYS = {"enter": "\r", "esc": "\x1b", "down": "\x1b[B", "up": "\x1b[A", "space": " ", "tab": "\t", "f2": "\x1bOQ"}
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "docs", "screenshots")
FONT = ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 14)
CW, CH = 9, 17
NAMED = {"black": "000000", "red": "ff5f5f", "green": "5fff87", "brown": "ffd75f", "blue": "5f87ff",
         "magenta": "d787ff", "cyan": "5fd7ff", "white": "e4e4e4", "default": None}


def color(c, default):
    if c in NAMED:
        c = NAMED[c] or default
    return "#" + c if len(c) == 6 and all(x in "0123456789abcdef" for x in c) else default


def png(screen, path):
    img = Image.new("RGB", (COLS * CW, ROWS * CH), "#1c1c1c")
    d = ImageDraw.Draw(img)
    for y in range(ROWS):
        for x in range(COLS):
            ch = screen.buffer[y][x]
            fg, bg = color(ch.fg, "#d0d0d0"), color(ch.bg, "#1c1c1c")
            if ch.reverse:
                fg, bg = bg, fg
            d.rectangle([x * CW, y * CH, (x + 1) * CW, (y + 1) * CH], fill=bg)
            if ch.data.strip():
                d.text((x * CW, y * CH), ch.data, font=FONT, fill=fg)
    img.save(path)


def main():
    subprocess.run(["cargo", "build", "-q"], cwd=ROOT, check=True)
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(ROOT)
        os.environ["TERM"] = "xterm-256color"
        os.execv("target/debug/termusic", ["termusic"])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    screen = pyte.Screen(COLS, ROWS)
    stream = pyte.ByteStream(screen)

    def pump(secs):
        end = time.time() + secs
        while time.time() < end:
            if select.select([fd], [], [], 0.1)[0]:
                try:
                    stream.feed(os.read(fd, 65536))
                except OSError:
                    return

    for step in sys.argv[1:]:
        kind, _, arg = step.partition(":")
        if kind == "w":
            pump(float(arg))
        elif kind == "t":
            os.write(fd, arg.encode()); pump(0.3)
        elif kind == "k":
            os.write(fd, KEYS.get(arg, arg).encode()); pump(0.4)
        elif kind == "snap":
            pump(0.3)
            text = "\n".join("".join(screen.buffer[y][x].data or " " for x in range(COLS)).rstrip() for y in range(ROWS))
            open(os.path.join(OUT, arg + ".txt"), "w").write(text + "\n")
            png(screen, os.path.join(OUT, arg + ".png"))
            print("saved", arg)
    try:
        os.write(fd, b"\x03"); pump(1.5)
    except OSError:
        pass  # app already quit
    time.sleep(2)  # watchdog polls every second
    left = subprocess.run(["pgrep", "-fl", "mpv --no-video"], capture_output=True, text=True).stdout.strip()
    print("mpv left running:", left or "none")


main()

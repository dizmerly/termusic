"""Live check of the pause-sync and loop fixes: real termusic in a pty, real mpv."""
import fcntl, glob, json, os, pty, select, socket, struct, subprocess, sys, tempfile, termios, time

BIN = sys.argv[1]
cfg = tempfile.mkdtemp(prefix="termusic-live-")
json.dump({"volume": 5}, open(os.path.join(cfg, "settings.json"), "w"))
env = dict(os.environ, TERMUSIC_CONFIG_DIR=cfg, TERM="xterm-256color")

pid, fd = pty.fork()
if pid == 0:
    os.execve(BIN, [BIN], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 110, 0, 0))


def read(secs):
    out, end = b"", time.time() + secs
    while time.time() < end:
        if select.select([fd], [], [], 0.1)[0]:
            try:
                out += os.read(fd, 65536)
            except OSError:
                break
    return out.decode("utf-8", "replace")


def key(s):
    os.write(fd, s.encode())


def socks():
    return glob.glob(os.path.join(tempfile.gettempdir(), f"termusic-mpv-{pid}-*.sock"))


def ipc(cmd):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(1)
    s.connect(socks()[0])
    s.sendall((json.dumps({"command": cmd, "request_id": 9}) + "\n").encode())
    buf = b""
    while b'"request_id":9' not in buf:
        buf += s.recv(4096)
    s.close()
    for line in buf.decode().splitlines():
        v = json.loads(line)
        if v.get("request_id") == 9:
            return v.get("data")


ok = True


def check(name, cond, detail=""):
    global ok
    ok &= bool(cond)
    print(("PASS " if cond else "FAIL ") + name, detail)


try:
    read(2)
    key("never gonna give you up official video\r")
    read(12)
    key("\r")
    for _ in range(40):  # wait for mpv to be playing
        read(1)
        try:
            if socks() and (ipc(["get_property", "time-pos"]) or 0) > 1:
                break
        except Exception:
            pass
    check("track is playing", socks() and ipc(["get_property", "pause"]) is False)

    # Bug 1: pause from outside the app (stands in for a headphone disconnect).
    read(0.5)
    ipc(["set_property", "pause", True])
    out = read(2.5)
    check("TUI shows the pause icon after an external pause", "⏸" in out)
    key("p")
    out = read(2.5)
    check("p resumes mpv (real state was paused)", ipc(["get_property", "pause"]) is False)
    check("TUI shows the play icon after p", "▶" in out and "⏸" not in out)
    key("p")
    out = read(2.5)
    check("second p pauses mpv and shows pause", ipc(["get_property", "pause"]) is True and "⏸" in out)
    key("p")
    read(2.5)

    # Bug 2: one press of l repeats the current track.
    key("l")
    out = read(1.5)
    check("one l press shows loop-one", "loop: this track" in out and "↻1" in out)
    check("mpv loop-file is inf", ipc(["get_property", "loop-file"]) == "inf")
    dur = ipc(["get_property", "duration"])
    ipc(["set_property", "time-pos", dur - 5])
    read(11)
    pos = ipc(["get_property", "time-pos"])
    check("track wrapped to its start", pos is not None and pos < 15, f"pos={pos} dur={dur}")
    check("still one mpv socket", len(socks()) == 1, str(socks()))
finally:
    key("q")
    read(2)
    try:
        os.kill(pid, 9)
    except OSError:
        pass
    time.sleep(1)
    left = subprocess.run(["pgrep", "-f", f"termusic-mpv-{pid}-"], capture_output=True, text=True).stdout.split()
    check("no mpv left running", not left, str(left))
    for p in left:
        os.kill(int(p), 9)
sys.exit(0 if ok else 1)

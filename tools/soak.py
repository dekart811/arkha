#!/usr/bin/env python3
"""Soak test: runs Arkha for a while in a sealed-off session and does random
things with it, watching for crashes, GLib/GTK warnings, hangs, slow
responses and growing memory.

    tools/soak.py --backend x11 --minutes 30
    tools/soak.py --backend wayland --minutes 30 --seed 7
    tools/soak.py --backend x11 --minutes 10 --heaptrack   (memory leaks)

Nothing of the user's is used or even visible: the run is in a bubblewrap
sandbox where the whole system is read-only, /home, /mnt, /media and
/run/user are empty, the home folder (with generated files) is a 1 GiB
tmpfs, and only target/soak/<run>/ (logs, report) is written. It gets its
own D-Bus session (so its own GVfs, Trash and settings daemons), its own
network namespace and a headless display: Xvfb for X11, and for Wayland
Weston running in an Xvfb (for a seat: keyboard and pointer). Nothing
shows on the screen.

Arkha is driven through its actions over D-Bus (org.gtk.Actions), as the
menus and shortcuts call them, and by keyboard and mouse input (clicks,
drags, the wheel, keys, typing; xdotool in the run's own X server, on
Wayland into Weston's window). Its dialogs (questions, conflicts, errors,
file choosers) are answered through accessibility (AT-SPI) by pressing one
of their buttons, on X11 and Wayland alike. Windows without buttons
(Preferences, Properties) are only opened on X11, where Escape closes
them. Meanwhile another thread keeps changing files in one folder.

No other program can be started by opening a file: the folders listing
applications are empty in the sandbox, so Arkha finds none to open files
with, and a question whether to run a program is never answered Run.

The report goes to target/soak/<run>/report.txt (and is printed); the exit
status is 1 if the run found a problem.
"""

import argparse
import os
import random
import re
import shutil
import signal
import stat
import subprocess
import sys
import threading
import time
import zlib
import struct

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = "io.github.dekart811.Arkha"
APP_PATH = "/io/github/dekart811/Arkha"

# Buttons that answer a dialog (rather than, in a file chooser, go
# somewhere): one of these is pressed when there are any.
ANSWERS = {"Cancel", "Skip", "Skip All", "Keep Both", "Merge", "Replace", "Delete", "Delete Permanently", "Move to Trash",
           "Empty Trash", "OK", "Close", "Rename", "Select", "Retry", "Copy", "Move", "Add", "Remove", "Yes", "No"}
# Buttons that run a program (asked about an executable file): never pressed.
RUN_BUTTONS = {"Run", "Run in Terminal"}
# The window cancel_operations looks for.
OPS_TITLE = "File Operations"


def localize(language):
    """For a run in another language (--language): the answers, the run
    buttons and the operations window in it too, from Arkha's and GTK's
    catalogs (button labels have mnemonics: compared without "_")."""
    global OPS_TITLE
    import gettext
    strip = lambda t: t.replace("_", "")
    for domain, d in (("arkha", "/tmp/arkha/locale"), ("gtk30", "/usr/share/locale")):
        try:
            cat = gettext.translation(domain, d, languages=[language])._catalog
        except OSError:
            continue
        for msgid, msgstr in list(cat.items()):
            if not isinstance(msgid, str) or not msgid:
                continue
            if strip(msgid) in ANSWERS:
                ANSWERS.add(strip(msgstr))
            if domain == "arkha" and strip(msgid) in ("Run", "Run in Terminal"):
                RUN_BUTTONS.add(strip(msgstr))
            if domain == "arkha" and msgid == "File Operations":
                OPS_TITLE = msgstr


def is_run(name):
    return name.replace("_", "") in RUN_BUTTONS

# Window actions: (name, parameter kind, weight, kind). Kinds: "plain" (no
# dialog), "dialog" (opens one with buttons, answered through AT-SPI), and
# "x11" (a window without buttons, closed with Escape: X11 only).
# "input": keyboard and mouse input (see do_input), not an action.
# Left out: what starts other programs whatever the sandbox hides
# (terminal, compress, extract, custom-action); windows are closed now and then below
# (not close-all-windows: Arkha quits, which would look like a crash).
ACTIONS = [
    ("back", None, 4, "plain"), ("forward", None, 3, "plain"), ("up", None, 4, "plain"),
    ("home", None, 2, "plain"), ("go-desktop", None, 1, "plain"),
    ("go-trash", None, 2, "plain"), ("go-uri", "uri", 12, "plain"), ("reload", None, 3, "plain"),
    ("new-tab", None, 3, "plain"), ("close-tab", None, 2, "plain"), ("next-tab", None, 2, "plain"),
    ("prev-tab", None, 1, "plain"), ("reopen-tab", None, 1, "plain"), ("new-window", None, 1, "plain"),
    ("view-mode", "view", 6, "plain"), ("zoom-in", None, 3, "plain"), ("zoom-out", None, 3, "plain"),
    ("zoom-normal", None, 1, "plain"), ("show-hidden", "toggle", 2, "plain"), ("show-sidebar", "toggle", 1, "plain"),
    ("show-statusbar", "toggle", 1, "plain"), ("lock-bar", "toggle", 1, "plain"),
    ("show-toolbar", "toggle", 1, "plain"), ("show-locbar", "toggle", 1, "plain"), ("reset-view", None, 1, "plain"),
    ("stop", None, 1, "plain"), ("clear-history", None, 1, "plain"), ("tab-left", None, 1, "plain"), ("tab-right", None, 1, "plain"),
    ("switch-pane", None, 1, "plain"), ("same-location", None, 1, "plain"), ("split-view", "toggle", 2, "plain"), ("swap-panes", None, 1, "plain"), ("info-panel", "toggle", 2, "plain"),
    ("sort-by", "sort", 3, "plain"), ("sort-desc", "toggle", 2, "plain"), ("folders-first", "toggle", 1, "plain"),
    ("filter", None, 1, "plain"), ("location", None, 1, "plain"), ("escape", None, 3, "plain"),
    ("select-all", None, 5, "plain"), ("invert-selection", None, 3, "plain"),
    ("copy", None, 4, "plain"), ("cut", None, 3, "plain"), ("paste", None, 4, "plain"), ("paste-check", None, 1, "plain"), ("paste-into", None, 1, "plain"),
    # Undoes the last copy, move or link, whatever happened to it since.
    ("undo", None, 3, "plain"), ("redo", None, 2, "plain"),
    ("duplicate", None, 2, "plain"), ("make-link", None, 1, "plain"), ("copy-to-pane", None, 1, "plain"),
    ("move-to-pane", None, 1, "plain"), ("new-folder", None, 2, "plain"), ("new-file", None, 2, "plain"),
    ("new-from-template", "template", 1, "plain"), ("restore", None, 1, "plain"),
    ("open-parent", None, 1, "plain"), ("add-bookmark", None, 1, "plain"),
    ("rename", None, 1, "dialog"), ("rename-all", None, 1, "dialog"), ("select-pattern", None, 1, "dialog"),
    ("edit-bookmarks", None, 1, "dialog"), ("about", None, 1, "dialog"), ("copy-to", None, 1, "dialog"),
    ("connect-server", None, 1, "dialog"),
    ("move-to", None, 1, "dialog"), ("delete", None, 2, "dialog"), ("empty-trash", None, 1, "dialog"),
    # Asks before deleting where there's no trash (the Trash itself, /usr).
    ("trash", None, 3, "dialog"),
    ("properties", None, 2, "x11"), ("preferences", None, 1, "x11"), ("shortcuts", None, 1, "x11"),
    # Opening: folders open, files ask which program (none there to pick).
    ("open", None, 2, "dialog"), ("open-new-tab", None, 1, "dialog"), ("open-new-window", None, 1, "dialog"),
    ("open-with-other", None, 1, "dialog"),
    ("backgrounds", None, 1, "dialog"), ("visible-columns", None, 1, "dialog"),
    ("click", None, 8, "input"), ("double-click", None, 4, "input"), ("right-click", None, 4, "input"),
    ("middle-click", None, 2, "input"), ("drag", None, 4, "input"), ("wheel", None, 3, "input"),
    ("keys", None, 6, "input"), ("typeahead", None, 3, "input"), ("rename-inline", None, 2, "input"),
    ("side-button", None, 1, "input"), ("search", None, 2, "input"), ("location-typed", None, 1, "input"),
]

# What the "keys" input presses (one to four of them).
KEYS = ["Up", "Down", "Left", "Right", "Home", "End", "Page_Up", "Page_Down", "space", "shift+Down", "shift+End", "ctrl+space",
        "Return", "BackSpace", "Escape", "Tab", "Delete", "alt+Up", "alt+Left", "alt+Right", "Menu", "shift+F10", "ctrl+plus", "ctrl+minus"]


# ------------------------------------------------------------------ fixture

def png_bytes(w, h, alpha, seed):
    """A small PNG: a colour gradient, with transparency if `alpha`."""
    rnd = random.Random(seed)
    base = [rnd.randrange(256) for _ in range(3)]
    rows = []
    for y in range(h):
        row = bytearray([0])
        for x in range(w):
            px = [(base[0] + x * 3) % 256, (base[1] + y * 3) % 256, base[2]]
            if alpha:
                px.append((x * 255) // max(1, w - 1))
            row.extend(px)
        rows.append(bytes(row))
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6 if alpha else 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b"")


def make_fixture(home, rnd):
    files = os.path.join(home, "Files")
    for d in ["Desktop", "Documents", "Downloads", "Music", "Pictures", "Videos", "Templates", "Files/churn"]:
        os.makedirs(os.path.join(home, d), exist_ok=True)
    with open(os.path.join(home, ".config/user-dirs.dirs"), "w") as f:
        for k, v in [("DESKTOP", "Desktop"), ("DOCUMENTS", "Documents"), ("DOWNLOAD", "Downloads"), ("MUSIC", "Music"),
                     ("PICTURES", "Pictures"), ("VIDEOS", "Videos"), ("TEMPLATES", "Templates"), ("PROJECTS", "Files")]:
            f.write(f'XDG_{k}_DIR="$HOME/{v}"\n')
    with open(os.path.join(home, "Templates/Empty Text.txt"), "w") as f:
        f.write("")
    # A big folder.
    many = os.path.join(files, "many")
    os.makedirs(many)
    exts = [".txt", ".jpg", ".png", ".tar.gz", ".mp4", "", ".PDF", ".rs", "~", ".zip"]
    for i in range(1500):
        name = f"file{i}{rnd.choice(exts)}"
        if rnd.random() < 0.05:
            name = "." + name
        with open(os.path.join(many, name), "w") as f:
            f.write("x" * rnd.randrange(0, 4000))
    # A tree.
    def tree(path, depth):
        os.makedirs(path, exist_ok=True)
        for i in range(5):
            with open(os.path.join(path, f"f{i}.txt"), "w") as f:
                f.write(f"{path} {i}\n" * rnd.randrange(1, 50))
        if depth:
            for i in range(3):
                tree(os.path.join(path, f"sub{i}"), depth - 1)
    tree(os.path.join(files, "tree"), 4)
    # Awkward names.
    odd = os.path.join(files, "odd names")
    os.makedirs(odd)
    for name in ["ä ö ü ß.txt", "日本語のファイル.txt", "emoji 😀🎉.txt", "new\nline.txt", "tab\there.txt", "  spaces  ",
                 "-dash-start", "%20percent.txt", "back\\slash", "quote\"'.txt", "a" * 250 + ".txt", "‮right-to-left.txt",
                 "écombining.txt", "no-extension", ".hidden", "backup~", "UPPER.TXT", "file.with.many.dots.tar.gz"]:
        with open(os.path.join(odd, name), "w") as f:
            f.write(name)
    with open(os.path.join(odd, "bad-utf8-\udcff.txt".encode("utf-8", "surrogateescape").decode("utf-8", "surrogateescape")), "w") as f:
        f.write("not UTF-8")
    # Pictures, and videos if ffmpeg is there, for thumbnails.
    media = os.path.join(files, "media")
    os.makedirs(media)
    for i in range(40):
        with open(os.path.join(media, f"picture{i}.png"), "wb") as f:
            f.write(png_bytes(rnd.randrange(16, 300), rnd.randrange(16, 300), i % 3 == 0, i))
    with open(os.path.join(media, "broken.png"), "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\nnot really")
    with open(os.path.join(media, "empty.jpg"), "wb") as f:
        pass
    if shutil.which("ffmpeg"):
        for i in range(3):
            subprocess.run(["ffmpeg", "-loglevel", "error", "-f", "lavfi", "-i", f"testsrc=duration=1:size=160x120:rate=5",
                            "-pix_fmt", "yuv420p", os.path.join(media, f"clip{i}.mp4")], check=False)
    # Two small files the cycle rounds copy and undo.
    os.makedirs(os.path.join(files, "undo"))
    os.makedirs(os.path.join(files, "undo-dest"))
    for name in ["one.txt", "two.txt"]:
        with open(os.path.join(files, "undo", name), "w") as f:
            f.write(name)
    # Links, including dangling and looping ones.
    links = os.path.join(files, "links")
    os.makedirs(links)
    os.symlink("../tree", os.path.join(links, "to-tree"))
    os.symlink("/no/such/place", os.path.join(links, "dangling"))
    os.symlink("loop-b", os.path.join(links, "loop-a"))
    os.symlink("loop-a", os.path.join(links, "loop-b"))
    os.symlink(".", os.path.join(links, "self"))
    # Things that can't be read, and a FIFO (reading one would wait forever).
    locked = os.path.join(files, "locked")
    os.makedirs(os.path.join(locked, "closed"))
    with open(os.path.join(locked, "closed/inside.txt"), "w") as f:
        f.write("x")
    with open(os.path.join(locked, "secret.txt"), "w") as f:
        f.write("secret")
    os.chmod(os.path.join(locked, "secret.txt"), 0)
    os.chmod(os.path.join(locked, "closed"), 0)
    os.mkfifo(os.path.join(files, "special-fifo"))
    # A big (sparse) file.
    with open(os.path.join(files, "big.bin"), "wb") as f:
        f.truncate(8 * 1024 * 1024)
    return files


def unlock(path):
    """Makes everything under `path` removable again."""
    for dirpath, dirnames, filenames in os.walk(path):
        for n in dirnames + filenames:
            p = os.path.join(dirpath, n)
            if not os.path.islink(p):
                try:
                    os.chmod(p, 0o755 if os.path.isdir(p) else 0o644)
                except OSError:
                    pass
    for dirpath, dirnames, filenames in os.walk(path):
        for n in dirnames:
            p = os.path.join(dirpath, n)
            if not os.path.islink(p):
                try:
                    os.chmod(p, 0o755)
                except OSError:
                    pass


# ------------------------------------------------------------------ outer

def build():
    """Arkha with debug info and symbols, so backtraces and heaptrack name
    its functions (in its own target folder: the release build stays)."""
    env = dict(os.environ, CARGO_PROFILE_RELEASE_DEBUG="1", CARGO_PROFILE_RELEASE_STRIP="false")
    subprocess.run(["cargo", "build", "--release", "--target-dir", os.path.join(ROOT, "target/soak-build")], cwd=ROOT, env=env, check=True)
    return os.path.join(ROOT, "target/soak-build/release/arkha")


# Inside the sandbox (see outer).
HOME = "/home/soak"
RUN_DIR = "/tmp/soak-run"
HOME_SIZE = 1 << 30


def outer(args):
    """Starts the run inside a bubblewrap sandbox: the whole system
    read-only, the real /home, /mnt, /media and /run/user replaced by empty
    folders (the user's files and mounts aren't even visible), the home a
    size-limited tmpfs, and its own network namespace (so its X server and
    D-Bus can't meet the session's). Only the run's log folder in
    target/soak/ is written on disk."""
    if not args.binary:
        args.binary = build()
    run = os.path.join(ROOT, "target", "soak", f"{args.backend}-seed{args.seed}" + (f"-{args.name}" if args.name else ""))
    if os.path.exists(run):
        unlock(run)
        shutil.rmtree(run)
    os.makedirs(run)
    keep = {"PATH", "LANG", "LANGUAGE", "TERM", "TZ"} | {k for k in os.environ if k.startswith(("LC_", "ARKHA_"))} | {"LLVM_PROFILE_FILE"}
    env = {k: v for k, v in os.environ.items() if k in keep}
    env.update({
        "HOME": HOME,
        "XDG_CONFIG_HOME": HOME + "/.config",
        "XDG_DATA_HOME": HOME + "/.local/share",
        "XDG_CACHE_HOME": HOME + "/.cache",
        "XDG_RUNTIME_DIR": "/tmp/run",
        "GSETTINGS_BACKEND": "memory",
        "GVFS_DISABLE_FUSE": "1",
        "RUST_BACKTRACE": "1",
    })
    sandbox = ["bwrap", "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--unshare-pid", "--unshare-net", "--unshare-ipc",
               "--die-with-parent", "--new-session"]
    # No applications to start: opening a file only asks which to use.
    for d in ["/home", "/mnt", "/media", "/run/user", "/srv", "/usr/share/applications", "/usr/local/share/applications",
              "/var/lib/flatpak/exports/share/applications"]:
        if os.path.isdir(d):
            sandbox += ["--tmpfs", d]
    if os.path.isdir("/run/media"):
        sandbox += ["--tmpfs", "/run/media"]
    sandbox += ["--size", str(128 << 20), "--tmpfs", "/tmp", "--perms", "0700", "--dir", "/tmp/run",
                "--size", str(64 << 20), "--tmpfs", "/dev/shm",
                "--size", str(HOME_SIZE), "--tmpfs", HOME,
                "--bind", run, RUN_DIR,
                "--ro-bind", args.binary, "/tmp/arkha/arkha",
                # The translations built beside it (see src/i18n.rs).
                *(["--ro-bind", locale, "/tmp/arkha/locale"] if os.path.isdir(locale := os.path.join(os.path.dirname(os.path.abspath(args.binary)), "locale")) else []),
                "--ro-bind", os.path.abspath(__file__), "/tmp/arkha/soak.py",
                "--chdir", RUN_DIR]
    inner = ["dbus-run-session", "--", sys.executable, "/tmp/arkha/soak.py", "--inner", "--binary", "/tmp/arkha/arkha"]
    passed = [a for a in sys.argv[1:] if not a.startswith("--binary")]
    if "--binary" in sys.argv:
        i = sys.argv.index("--binary")
        passed = sys.argv[1:i] + sys.argv[i + 2:]
    # The private session's own messages (portals finding no display, …).
    with open(os.path.join(run, "session.log"), "w") as log:
        r = subprocess.run(sandbox + inner + passed, env=env, stderr=log)
    # The driver itself failing (a Python error) leaves a report of nothing.
    with open(os.path.join(run, "session.log")) as log:
        failed = "Traceback (most recent call last)" in log.read()
    if failed:
        msg = "PROBLEM: the soak driver itself failed: see the traceback in session.log"
        print(msg)
        with open(os.path.join(run, "report.txt"), "a") as rep:
            rep.write(msg + "\n")
    print(f"Logs and report: {run}")
    return 1 if failed else r.returncode


# ------------------------------------------------------------------ inner

class Run:
    def __init__(self, args):
        import gi
        gi.require_version("Gio", "2.0")
        gi.require_version("Atspi", "2.0")
        from gi.repository import Gio, GLib, Atspi
        self.Gio, self.GLib, self.Atspi = Gio, GLib, Atspi
        self.args = args
        self.dir = RUN_DIR
        self.home = os.environ["HOME"]
        assert self.home == HOME and os.path.isdir(RUN_DIR), "not in the sandbox"
        self.rnd = random.Random(args.seed)
        if args.language:
            localize(args.language)
        # The generated files, in the (tmpfs) home.
        for d in [".config/arkha", ".local/share", ".cache"]:
            os.makedirs(os.path.join(self.home, d), exist_ok=True)
        # Never asking before trashing: nothing could answer on Wayland.
        # Every list column and some icon captions, so all of them are drawn.
        with open(os.path.join(self.home, ".config/arkha/arkha.conf"), "w") as f:
            f.write("[General]\nconfirm_trash=false\n"
                    f"language={args.language}\n"
                    "list_columns=size;type;mtime;atime;btime;owner;group;permissions;octal;mime;extension;location;disk;\n"
                    "icon_captions=owner;permissions;extension;\n")
        self.files = make_fixture(self.home, random.Random(args.seed))
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.procs = []
        self.problems = []
        self.counts = {}
        self.latencies = []
        self.rss = []
        self.stop = threading.Event()
        self.timeline = []
        self.closes_failed = 0
        self.tabs, self.most_tabs, self.tab_closes_failed = None, 0, 0
        self.unanswered = set()
        self.started = time.monotonic()
        self.done = 0
        self.churned = 0

    def note(self, msg):
        line = f"[{time.strftime('%H:%M:%S')}] {msg}"
        print(line, flush=True)
        with open(os.path.join(self.dir, "events.txt"), "a") as f:
            f.write(line + "\n")

    def problem(self, msg):
        self.problems.append(msg)
        self.note("PROBLEM: " + msg)

    # -- display and app

    def start_display(self):
        env = os.environ
        # An X server either way: on Wayland, Weston runs in it (its x11
        # backend), as its headless backend has no seat (no keyboard or
        # pointer), and GTK's Wayland clipboard crashes without one.
        r, w = os.pipe()
        p = subprocess.Popen(["Xvfb", "-displayfd", str(w), "-screen", "0", "1280x800x24", "-nolisten", "tcp"],
                             pass_fds=[w], stdout=open(os.path.join(self.dir, "xvfb.log"), "w"), stderr=subprocess.STDOUT)
        os.close(w)
        num = os.read(r, 32).decode().strip()
        os.close(r)
        self.procs.append(p)
        env["DISPLAY"] = f":{num}"
        if self.args.backend == "x11":
            env["GDK_BACKEND"] = "x11"
        else:
            p = subprocess.Popen(["weston", "--backend=x11", "--socket=wayland-soak", "--idle-time=0", "--width=1280", "--height=800"],
                                 stdout=open(os.path.join(self.dir, "weston.log"), "w"), stderr=subprocess.STDOUT)
            sock = os.path.join(env["XDG_RUNTIME_DIR"], "wayland-soak")
            for _ in range(100):
                if os.path.exists(sock):
                    break
                time.sleep(0.05)
            env["WAYLAND_DISPLAY"] = "wayland-soak"
            env["GDK_BACKEND"] = "wayland"
        self.procs.append(p)

    def start_a11y(self):
        """The accessibility bus and registry (started by hand: their D-Bus
        services go through systemd, which the private session hasn't)."""
        for cmd in (["/usr/lib/at-spi-bus-launcher", "--launch-immediately"], ["/usr/lib/at-spi2-registryd"]):
            self.procs.append(subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=open(os.path.join(self.dir, "a11y.log"), "a")))
            time.sleep(0.5)
        self.Atspi.init()

    def start_arkha(self):
        binary = self.args.binary
        cmd = [binary, self.files]
        if self.args.heaptrack:
            cmd = ["heaptrack", "--record-only", "-o", os.path.join(self.dir, "heaptrack")] + cmd
        else:
            # Under gdb from the start (attaching later isn't allowed with
            # ptrace_scope 1): a crash prints its backtrace, and so does a
            # hang, which is interrupted (see backtrace).
            # debuginfod off: the sandbox has no network, and gdb 18 has
            # crashed on it while stopping. The stopped thread's backtrace
            # comes first, in case listing every thread goes wrong.
            cmd = ["gdb", "-q", "-batch", "-ex", "set pagination off", "-ex", "set print thread-events off",
                   "-ex", "set debuginfod enabled off",
                   "-ex", "handle SIGPIPE nostop noprint pass", "-ex", "run",
                   "-ex", "bt 120", "-ex", "thread apply all bt 120", "-ex", "kill", "--args"] + cmd
        self.log = open(os.path.join(self.dir, "arkha.log"), "a")
        env = dict(os.environ)
        if self.args.fatal_warnings:
            env["G_DEBUG"] = "fatal-warnings"
        elif self.args.fatal_criticals:
            env["G_DEBUG"] = "fatal-criticals"
        if self.args.no_a11y:
            # Arkha without its accessibility bridge (dialogs then can't be
            # answered: for rounds without any).
            env["NO_AT_BRIDGE"] = "1"
        self.arkha = subprocess.Popen(cmd, stdout=self.log, stderr=subprocess.STDOUT, env=env)
        for _ in range(200):
            if self.windows():
                break
            time.sleep(0.05)
        self.pid = self.app_pid()
        self.note(f"Arkha started (pid {self.pid}) on {self.args.backend}")

    def app_pid(self):
        try:
            r = self.bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetConnectionUnixProcessID",
                                   self.GLib.Variant("(s)", (APP,)), None, self.Gio.DBusCallFlags.NONE, 2000, None)
            return r.unpack()[0]
        except Exception:
            return self.arkha.pid

    def alive(self):
        return self.arkha.poll() is None

    def windows(self):
        try:
            r = self.bus.call_sync(APP, APP_PATH + "/window", "org.freedesktop.DBus.Introspectable", "Introspect", None, None,
                                   self.Gio.DBusCallFlags.NONE, 5000, None)
            return re.findall(r'<node name="(\d+)"', r.unpack()[0])
        except Exception:
            return []

    def call(self, path, iface, method, params, timeout=30000):
        t = time.monotonic()
        try:
            self.bus.call_sync(APP, path, iface, method, params, None, self.Gio.DBusCallFlags.NONE, timeout, None)
            ok = True
        except Exception as e:
            ok = False
            err = str(e)
        dt = time.monotonic() - t
        self.latencies.append(dt)
        if dt > 2:
            self.note(f"slow: {method} {params} took {dt:.1f} s")
        if not ok and self.alive() and "Timeout" in err:
            self.problem(f"no answer to {method} {params} within {timeout // 1000} s (hung?)")
            self.backtrace("hang")
        return ok

    def activate(self, win, name, param):
        v = [] if param is None else [param]
        if self.args.log_actions:
            with open(os.path.join(self.dir, "actions.txt"), "a") as f:
                f.write(f"{time.monotonic() - self.started:9.2f} win {win}: {name} {param.unpack() if param is not None else ''}\n")
        return self.call(f"{APP_PATH}/window/{win}", "org.gtk.Actions", "Activate", self.GLib.Variant("(sava{sv})", (name, v, {})))

    def backtrace(self, why):
        """Interrupts Arkha; gdb (its parent) then writes every thread's
        backtrace to the log and ends it."""
        if self.args.heaptrack:
            return
        self.note(f"interrupting Arkha for a backtrace ({why}), see arkha.log")
        try:
            os.kill(self.pid, signal.SIGINT)
            self.arkha.wait(timeout=60)
        except (OSError, subprocess.TimeoutExpired):
            self.arkha.kill()

    # -- what's done

    def param(self, kind):
        V = self.GLib.Variant
        rnd = self.rnd
        if kind == "uri":
            dirs = []
            for dp, dn, fn in os.walk(self.files):
                dirs.append(dp)
                if len(dirs) > 300:
                    break
            target = rnd.choice(dirs + ["trash:///", self.home, "/usr/share/applications", "/no/such/folder", self.files + "/many"])
            if "://" not in target:
                target = self.Gio.File.new_for_path(target).get_uri()
            return V("s", target)
        if kind == "view":
            return V("s", rnd.choice(["icon", "list", "compact", "columns"]))
        if kind == "sort":
            return V("s", rnd.choice(["name", "size", "type", "mtime", "atime", "btime", "owner", "group", "permissions",
                                      "octal", "mime", "extension", "location", "disk", "trashed", "orig"]))
        if kind == "template":
            return V("s", self.Gio.File.new_for_path(os.path.join(self.home, "Templates/Empty Text.txt")).get_uri())
        if kind == "toggle":
            return None
        return None

    def dialogs(self):
        """Arkha's open dialogs, as AT-SPI sees them."""
        found = []
        try:
            desk = self.Atspi.get_desktop(0)
            for i in range(desk.get_child_count()):
                app = desk.get_child_at_index(i)
                if not app or app.get_name() != "arkha":
                    continue
                for j in range(app.get_child_count()):
                    w = app.get_child_at_index(j)
                    if w and w.get_role_name() in ("dialog", "alert", "file chooser"):
                        found.append(w)
        except Exception:
            pass
        return found

    def is_modal(self, w):
        try:
            return w.get_state_set().contains(self.Atspi.StateType.MODAL)
        except Exception:
            return False

    def describe_tree(self, a, depth=0, out=None, limit=60):
        """role 'name' of `a` and what's in it, for notes."""
        out = [] if out is None else out
        try:
            if len(out) < limit:
                out.append("  " * depth + f"{a.get_role_name()} {a.get_name()!r}")
                for i in range(min(a.get_child_count(), 40)):
                    c = a.get_child_at_index(i)
                    if c:
                        self.describe_tree(c, depth + 1, out, limit)
        except Exception as e:
            out.append(f"(error {e})")
        return "\n" + "\n".join(out) if depth == 0 else out

    def tab_count(self):
        """The tabs open in Arkha's windows (both panes of split view), as
        AT-SPI sees them; None when it can't tell. The file views aren't
        gone through (thousands of items)."""
        skip = {"table", "tree table", "tree", "list", "list box", "layered pane", "text", "menu bar", "tool bar", "page tab"}
        try:
            desk = self.Atspi.get_desktop(0)
            tabs, seen = 0, 0
            for i in range(desk.get_child_count()):
                app = desk.get_child_at_index(i)
                if not app or app.get_name() != "arkha":
                    continue
                stack = [w for w in (app.get_child_at_index(j) for j in range(app.get_child_count())) if w and w.get_role_name() == "frame"]
                while stack and seen < 3000:
                    a = stack.pop()
                    seen += 1
                    role = a.get_role_name()
                    if role == "page tab list":
                        tabs += sum(1 for k in range(a.get_child_count()) if a.get_child_at_index(k).get_role_name() == "page tab")
                    if role in skip:
                        continue
                    stack.extend(c for c in (a.get_child_at_index(k) for k in range(min(a.get_child_count(), 200))) if c)
            return tabs
        except Exception:
            return None

    def describe_dialogs(self):
        return [(w.get_role_name(), w.get_name(), [n for r, n, _ in self.buttons(w)]) for w in self.dialogs()]

    def describe_windows(self):
        out = []
        try:
            desk = self.Atspi.get_desktop(0)
            for i in range(desk.get_child_count()):
                app = desk.get_child_at_index(i)
                if not app:
                    continue
                wins = [(app.get_child_at_index(j).get_role_name(), app.get_child_at_index(j).get_name()) for j in range(app.get_child_count())]
                out.append((app.get_name(), wins))
        except Exception as e:
            out.append(("error", str(e)))
        return out

    def buttons(self, w):
        """(role, name, accessible) of the buttons and check boxes in `w`
        that can be pressed now."""
        S = self.Atspi.StateType
        found, stack = [], [w]
        while stack and len(found) < 80:
            a = stack.pop()
            try:
                role = a.get_role_name()
                if role in ("button", "push button", "check box"):
                    st = a.get_state_set()
                    if st.contains(S.SHOWING) and st.contains(S.SENSITIVE):
                        found.append((role, a.get_name() or "", a))
                for k in range(min(a.get_child_count(), 400)):
                    c = a.get_child_at_index(k)
                    if c:
                        stack.append(c)
            except Exception:
                pass
        return found

    def answer_dialogs(self, rounds=1, ending=False):
        """Presses a button in each open dialog (now and then ticking a
        check box first, as "Apply this action to all"); whether none are
        left. `ending`: Cancel or Close where there is one, to end what's
        asking (a copy of many files asks about each conflict in turn)."""
        for _ in range(rounds):
            ds = self.dialogs()
            if not ds:
                return True
            # A modal dialog holds the input: nothing else can close while
            # it's open, and modal dialogs run inside each other, so one
            # finishes only once those opened after it have. So a modal one
            # is answered first, the last listed (AT-SPI lists them in no
            # set order), as a person would answer the one in front.
            modal = [w for w in ds if self.is_modal(w)]
            # Pressing a button through AT-SPI sends no mouse or key event,
            # so a tooltip shown in the dialog would stay up while it closes
            # (GDK warns on Wayland); a person's click or key hides it first.
            # The pointer moves off to the screen's corner, as theirs would.
            self.xdo("mousemove", 1278, 798)
            for w in (modal or ds)[-1:]:
                bs = self.buttons(w)
                press = lambda a: a.get_action_iface().do_action(0)
                try:
                    checks = [a for r, n, a in bs if r == "check box"]
                    if checks and self.rnd.random() < 0.2:
                        press(self.rnd.choice(checks))
                    named = [(n, a) for r, n, a in bs if r != "check box" and n]
                    answers = [(n, a) for n, a in named if n.replace("_", "") in ANSWERS] or [(n, a) for n, a in named if not is_run(n)]
                    # Retrying what failed mostly fails again: only now and then.
                    if len(answers) > 1 and self.rnd.random() > 0.05:
                        answers = [(n, a) for n, a in answers if n.replace("_", "") != "Retry"] or answers
                    if not answers:
                        # Noted once per kind of dialog: what it has instead.
                        key = (w.get_role_name(), w.get_name())
                        if key not in self.unanswered:
                            self.unanswered.add(key)
                            self.note(f"no button to answer {key}: {self.describe_tree(w)}")
                    if ending:
                        answers = [(n, a) for n, a in answers if n.replace("_", "") in ("Cancel", "Close")] or answers
                    if answers:
                        n, a = self.rnd.choice(answers)
                        self.counts["answer:" + n.replace("_", "")] = self.counts.get("answer:" + n.replace("_", ""), 0) + 1
                        if self.args.log_actions:
                            with open(os.path.join(self.dir, "actions.txt"), "a") as f:
                                f.write(f"{time.monotonic() - self.started:9.2f} answer {w.get_role_name()} {w.get_name()!r}: {n}\n")
                        press(a)
                except Exception as e:
                    self.note(f"couldn't answer a dialog: {e}")
            if self.args.backend == "x11":
                self.close_dialogs()
            time.sleep(0.3)
        return not self.dialogs()

    def cancel_operations(self):
        """Presses the Cancel buttons of the File Operations window: file
        operations still running when Arkha is told to quit go on (with
        that window) until they're done, as a person would cancel them.
        How many were pressed."""
        pressed = 0
        try:
            desk = self.Atspi.get_desktop(0)
            for i in range(desk.get_child_count()):
                app = desk.get_child_at_index(i)
                if not app or app.get_name() != "arkha":
                    continue
                for j in range(app.get_child_count()):
                    w = app.get_child_at_index(j)
                    if w and w.get_name() == OPS_TITLE:
                        for _, _, a in self.buttons(w):
                            a.get_action_iface().do_action(0)
                            pressed += 1
        except Exception as e:
            self.note(f"couldn't cancel operations: {e}")
        return pressed

    def close_dialogs(self):
        """X11: Escape (or, for a question, sometimes Enter) in every dialog."""
        env = dict(os.environ)
        try:
            ids = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(self.pid), ""], env=env, capture_output=True,
                                 text=True, timeout=10).stdout.split()
        except subprocess.TimeoutExpired:
            return
        # Enter only where every dialog is seen through AT-SPI and none asks
        # whether to run a program (Run is what Enter would do).
        seen = self.dialogs()
        may_enter = bool(seen) and not any(is_run(n) for d in seen for _, n, _ in self.buttons(d))
        for w in ids:
            kind = subprocess.run(["xprop", "-id", w, "_NET_WM_WINDOW_TYPE", "WM_TRANSIENT_FOR"], env=env, capture_output=True, text=True).stdout
            if "DIALOG" not in kind and "window id #" not in kind:
                continue
            key = "Return" if may_enter and self.rnd.random() < 0.3 else "Escape"
            subprocess.run(["xdotool", "windowfocus", "--sync", w, "key", key], env=env, capture_output=True, timeout=10)

    # -- keyboard and mouse

    def xdo(self, *args):
        try:
            subprocess.run(["xdotool", *map(str, args)], env=os.environ, capture_output=True, timeout=10)
        except subprocess.TimeoutExpired:
            self.note(f"xdotool {' '.join(map(str, args))}: no answer within 10 s")

    def input_window(self):
        """(X window, width, height, top) to send input to: on X11 one of
        Arkha's main windows, raised and focused; on Wayland Weston's window
        (below its panel)."""
        env = os.environ
        try:
            if self.args.backend == "x11":
                ids = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(self.pid), ""], env=env, capture_output=True,
                                     text=True, timeout=10).stdout.split()
                mains = []
                for w in ids:
                    kind = subprocess.run(["xprop", "-id", w, "_NET_WM_WINDOW_TYPE", "WM_TRANSIENT_FOR"], env=env, capture_output=True,
                                          text=True, timeout=10).stdout
                    if "DIALOG" in kind or "window id #" in kind or "MENU" in kind or "TOOLTIP" in kind:
                        continue
                    mains.append(w)
                top = 0
            else:
                mains = subprocess.run(["xdotool", "search", "--classname", "weston"], env=env, capture_output=True, text=True,
                                       timeout=10).stdout.split()
                top = 40
            if not mains:
                return None
            w = self.rnd.choice(mains)
            geo = subprocess.run(["xdotool", "getwindowgeometry", "--shell", w], env=env, capture_output=True, text=True, timeout=10).stdout
            size = dict(l.split("=", 1) for l in geo.split() if "=" in l)
            width, height = int(size.get("WIDTH", 0)), int(size.get("HEIGHT", 0))
            if width < 300 or height < 200:
                return None
        except (subprocess.TimeoutExpired, ValueError):
            return None
        if self.args.backend == "x11":
            self.xdo("windowraise", w)
        self.xdo("windowfocus", "--sync", w)
        return w, width, height, top

    def do_input(self, name):
        """Keyboard or mouse input to a window, as a person would give it."""
        t = self.input_window()
        if not t:
            return
        w, width, height, top = t
        rnd = self.rnd
        point = lambda: (rnd.randrange(10, width - 10), rnd.randrange(top + 10, height - 10))
        x, y = point()
        if self.args.log_actions:
            with open(os.path.join(self.dir, "actions.txt"), "a") as f:
                f.write(f"{time.monotonic() - self.started:9.2f} input {name} at {x},{y}\n")
        move = ["mousemove", "--window", w, x, y]
        word = lambda n: "".join(rnd.choice("abcdefghijklmnopqrstuvwxyz0123456789 ._-") for _ in range(rnd.randint(1, n)))
        if name == "click":
            mods = rnd.choice([[], [], [], ["ctrl"], ["shift"]])
            for m in mods:
                self.xdo("keydown", m)
            self.xdo(*move, "click", 1)
            for m in mods:
                self.xdo("keyup", m)
        elif name == "double-click":
            self.xdo(*move, "click", "--repeat", 2, "--delay", 60, 1)
        elif name == "right-click":
            self.xdo(*move, "click", 3)
            time.sleep(0.4)
            # A few steps through the menu, never choosing: some items start
            # programs (a terminal).
            self.xdo("key", *(["Down"] * rnd.randint(0, 3)), "Escape")
        elif name == "middle-click":
            # A file or folder opens (a folder in a new tab); on nothing,
            # autoscroll, ended by moving and clicking.
            self.xdo(*move, "click", 2)
            time.sleep(0.5)
            x2, y2 = point()
            self.xdo("mousemove", "--window", w, x2, y2)
            time.sleep(0.3)
            self.xdo("click", 1)
        elif name == "drag":
            # Items dragged somewhere else in the window (another folder,
            # the side pane, the path bar): moved, or with Ctrl copied, with
            # Shift+Ctrl linked, with Alt asked (the menu closed again).
            mods = rnd.choice([[], [], ["ctrl"], ["shift", "ctrl"], ["alt"]])
            x2, y2 = point()
            self.xdo(*move, "mousedown", 1)
            for m in mods:
                self.xdo("keydown", m)
            for i in range(1, 11):
                self.xdo("mousemove", "--window", w, x + (x2 - x) * i // 10, y + (y2 - y) * i // 10)
                time.sleep(0.04)
            self.xdo("mouseup", 1)
            for m in mods:
                self.xdo("keyup", m)
            if "alt" in mods:
                time.sleep(0.4)
                self.xdo("key", "Escape")
        elif name == "wheel":
            ctrl = rnd.random() < 0.25
            if ctrl:
                self.xdo("keydown", "ctrl")
            self.xdo(*move, "click", "--repeat", rnd.randint(1, 8), "--delay", 30, rnd.choice([4, 5]))
            if ctrl:
                self.xdo("keyup", "ctrl")
        elif name == "keys":
            keys = [rnd.choice(KEYS) for _ in range(rnd.randint(1, 4))]
            if any(k in ("Menu", "shift+F10") for k in keys):
                keys.append("Escape")
            self.xdo("key", "--delay", 50, *keys)
        elif name == "typeahead":
            self.xdo(*move, "click", 1)
            self.xdo("type", "--delay", 60, word(3))
            time.sleep(0.3)
            self.xdo("key", rnd.choice(["Down", "Up", "Escape", "Escape"]))
        elif name == "rename-inline":
            self.xdo("key", "F2")
            time.sleep(0.4)
            if rnd.random() < 0.3:
                self.xdo("key", "ctrl+a")
            # Now and then a slash (refused) or nothing.
            text = rnd.choice([word(12), word(12), word(12), "a/b", ""])
            if text:
                self.xdo("type", "--delay", 30, text)
            self.xdo("key", rnd.choice(["Return", "Return", "Return", "Escape"]))
        elif name == "side-button":
            self.xdo(*move, "click", rnd.choice([8, 9]))
        elif name == "search":
            self.xdo("key", "ctrl+f")
            time.sleep(0.3)
            self.xdo("type", "--delay", 40, rnd.choice(["a", "file", "txt", "sub", "zz", word(4)]))
            self.xdo("key", "Return")
            time.sleep(rnd.uniform(0.3, 2))
            if rnd.random() < 0.5:
                self.xdo("key", "Escape")
        elif name == "location-typed":
            self.xdo("key", "ctrl+l")
            time.sleep(0.3)
            self.xdo("key", "ctrl+a")
            target = rnd.choice([self.files + "/", self.files + "/tr", self.home + "/", "/usr/sh", "~/", "/no/such", "trash:///"])
            self.xdo("type", "--delay", 30, target)
            time.sleep(0.4)
            self.xdo("key", rnd.choice(["Return", "Return", "Tab", "Escape"]))

    def churn(self):
        """Keeps changing files in Files/churn, as other programs would."""
        d = os.path.join(self.files, "churn")
        rnd = random.Random(self.args.seed + 1)
        while not self.stop.is_set():
            try:
                # The driver may have moved it away (cut and paste).
                os.makedirs(d, exist_ok=True)
                names = os.listdir(d)
                r = rnd.random()
                if r < 0.35 or not names:
                    with open(os.path.join(d, f"c{rnd.randrange(10**6)}.txt"), "w") as f:
                        f.write("x" * rnd.randrange(100))
                elif r < 0.55:
                    p = os.path.join(d, rnd.choice(names))
                    if os.path.isdir(p) and not os.path.islink(p):
                        shutil.rmtree(p, ignore_errors=True)
                    else:
                        os.remove(p)
                elif r < 0.7:
                    os.rename(os.path.join(d, rnd.choice(names)), os.path.join(d, f"r{rnd.randrange(10**6)}"))
                elif r < 0.85:
                    p = os.path.join(d, rnd.choice(names))
                    if os.path.isfile(p):
                        with open(p, "a") as f:
                            f.write("more")
                elif r < 0.97:
                    os.makedirs(os.path.join(d, f"dir{rnd.randrange(1000)}"), exist_ok=True)
                else:
                    # A burst, as an unpacked archive.
                    burst = os.path.join(d, f"burst{rnd.randrange(1000)}")
                    os.makedirs(burst, exist_ok=True)
                    for i in range(300):
                        open(os.path.join(burst, f"b{i}"), "w").close()
                    if rnd.random() < 0.5:
                        shutil.rmtree(burst, ignore_errors=True)
                self.churned += 1
            except OSError:
                pass
            time.sleep(rnd.uniform(0.02, 0.3))

    def cycles(self, end):
        """The same round of actions again and again, memory measured after
        each: once warmed up it should stop growing (growth that goes on
        round after round is memory never freed)."""
        uri = lambda p: self.GLib.Variant("s", self.Gio.File.new_for_path(os.path.join(self.files, p)).get_uri())
        s = self.GLib.Variant("s", "")
        round_ = [
            # A copy into another folder (so select-all never finds the
            # copies), undone at the end of the round once it's done.
            ("go-uri", uri("undo")), ("select-all", None), ("copy", None), ("go-uri", uri("undo-dest")), ("paste", None),
            ("go-uri", uri("many")), ("view-mode", self.GLib.Variant("s", "icon")), ("select-all", None), ("go-uri", uri("media")),
            ("view-mode", self.GLib.Variant("s", "list")), ("new-tab", None), ("go-uri", uri("tree")), ("view-mode", self.GLib.Variant("s", "columns")),
            ("close-tab", None), ("view-mode", self.GLib.Variant("s", "compact")), ("go-uri", uri("odd names")), ("zoom-in", None),
            ("zoom-out", None), ("split-view", None), ("go-uri", uri("tree/sub0")), ("split-view", None), ("info-panel", None),
            ("go-uri", uri("media")), ("info-panel", None), ("show-hidden", None), ("show-hidden", None), ("back", None), ("forward", None),
            ("go-uri", uri("churn")), ("reload", None), ("escape", None), ("lock-bar", None), ("lock-bar", None),
            # The copy made at the start taken back (the copies go to the trash).
            ("undo", None),
        ]
        del s
        if self.args.round:
            # A round of one's own: "new-tab,go-uri:tree,view-mode:list,close-tab".
            round_ = []
            for a in self.args.round.split(","):
                name, _, arg = a.partition(":")
                param = None
                if name == "go-uri":
                    param = uri(arg)
                elif arg:
                    param = self.GLib.Variant("s", arg)
                round_.append((name, param))
        self.cycle_rss = []
        n = 0
        while time.monotonic() < end and self.alive():
            wins = self.windows()
            if not wins:
                self.call(APP_PATH, "org.gtk.Application", "Activate", self.GLib.Variant("(a{sv})", ({},)))
                time.sleep(0.5)
                continue
            for name, param in round_:
                self.activate(wins[0], name, param)
                self.counts[name] = self.counts.get(name, 0) + 1
                n += 1
                self.done = n
                time.sleep(0.05)
            self.answer_dialogs()
            time.sleep(0.5)
            self.sample_rss()
            if self.rss:
                self.cycle_rss.append(self.rss[-1][1])
            if len(self.cycle_rss) % 25 == 0:
                self.note(f"round {len(self.cycle_rss)}: {self.cycle_rss[-1]} MB")
        return n

    def sample_rss(self):
        try:
            with open(f"/proc/{self.pid}/status") as f:
                for line in f:
                    if line.startswith("VmRSS:"):
                        self.rss.append((time.monotonic(), int(line.split()[1]) // 1024))
                        # For data.json: seconds since the start, MB, actions done, windows.
                        self.timeline.append((round(time.monotonic() - self.started, 1), self.rss[-1][1], self.done, len(self.windows())))
        except OSError:
            pass

    def arkha_frames(self):
        """Arkha's own frames of the first thread gdb shows stopped by a
        signal (a crash, a critical made fatal, an interrupted hang)."""
        text = open(os.path.join(self.dir, "arkha.log"), errors="replace").read()
        m = re.search(r"received signal (\w+).*?\n(#0.*?)(?:\n\n|\nThread )", text, re.S)
        if not m:
            return []
        frames = [re.sub(r"^#\d+\s+(0x[0-9a-f]+ in )?", "", l) for l in m.group(2).splitlines() if "arkha::" in l or " at src/" in l]
        return [f"(signal {m.group(1)})"] + frames[:15]

    def warnings(self):
        self.log.flush()
        found = {}
        with open(os.path.join(self.dir, "arkha.log"), errors="replace") as f:
            for line in f:
                if re.search(r"-CRITICAL \*\*|-WARNING \*\*|panicked at|Gtk-Message|Program received signal|exited with code", line):
                    key = re.sub(r"\d+", "N", line.strip())[:200]
                    found[key] = found.get(key, 0) + 1
        return found

    # -- the run

    def go(self):
        self.start_display()
        self.start_a11y()
        self.start_arkha()
        if not self.alive():
            self.problem("Arkha didn't start")
            return
        churn = threading.Thread(target=self.churn, daemon=True)
        churn.start()
        actions = [a for a in ACTIONS if a[3] != "x11" or self.args.backend == "x11"]
        weights = [a[2] for a in actions]
        end = time.monotonic() + self.args.minutes * 60
        last_rss = 0
        last_report = time.monotonic()
        n = 0
        if self.args.cycle:
            n = self.cycles(end)
        while time.monotonic() < end and not self.args.cycle:
            if not self.alive():
                # Closing its last window ends it, as it should: started
                # again, as the user would. Anything else is a problem.
                self.log.flush()
                tail = open(os.path.join(self.dir, "arkha.log"), errors="replace").read()[-2000:]
                if "exited normally" in tail and not self.args.heaptrack:
                    self.restarts = getattr(self, "restarts", 0) + 1
                    self.note("Arkha ended with its last window; starting it again")
                    with open(os.path.join(self.dir, "arkha.log"), "a") as f:
                        f.write("\n=== started again ===\n")
                    self.start_arkha()
                    continue
                self.problem(f"Arkha exited (status {self.arkha.returncode}) after {n} actions")
                break
            wins = self.windows()
            if not wins:
                # Every window was closed: as the desktop would, start one.
                self.call(APP_PATH, "org.gtk.Application", "Activate", self.GLib.Variant("(a{sv})", ({},)))
                time.sleep(0.3)
                continue
            if len(wins) > 6 and self.closes_failed < 5:
                # Plenty: close one instead of opening more. While a modal
                # dialog is open GTK ignores that: after a few tries, go on
                # with other actions (which answer dialogs now and then).
                self.activate(self.rnd.choice(wins), "close-window", None)
                time.sleep(0.2)
                self.answer_dialogs()
                if len(self.windows()) >= len(wins):
                    self.closes_failed += 1
                    if self.closes_failed == 5:
                        self.note(f"windows don't close; dialogs: {self.describe_dialogs()}")
                else:
                    self.closes_failed = 0
                continue
            if len(wins) <= 6:
                self.closes_failed = 0
            # Tabs open more ways than they close (clicks on folders,
            # Open in New Tab, Reopen): without this they piled up, ~140 in
            # 20 minutes, and memory grew with them (not a leak: --cycle
            # stays flat). Past 8 a window (on average) one is closed.
            if n % 10 == 0:
                self.tabs = self.tab_count()
                self.most_tabs = max(self.most_tabs, self.tabs or 0)
            if (self.tabs or 0) <= 8 * len(wins):
                self.tab_closes_failed = 0
            elif self.tab_closes_failed < 5:
                self.activate(self.rnd.choice(wins), "close-tab", None)
                time.sleep(0.2)
                self.answer_dialogs()
                before, self.tabs = self.tabs, self.tab_count()
                self.tab_closes_failed = self.tab_closes_failed + 1 if (self.tabs or 0) >= before else 0
                continue
            name, kind, _, how = self.rnd.choices(actions, weights)[0]
            win = self.rnd.choice(wins)
            if how == "plain":
                self.activate(win, name, self.param(kind))
            elif how == "input":
                self.do_input(name)
                # A dialog it opened (an error, a question) would keep the
                # next action waiting: answered first.
                for _ in range(10):
                    time.sleep(0.2)
                    if self.answer_dialogs():
                        break
            else:
                # The call returns once the dialog is closed (a modal one
                # runs its own loop): closed from here meanwhile.
                t = threading.Thread(target=self.activate, args=(win, name, self.param(kind)))
                t.start()
                for i in range(60):
                    time.sleep(0.3)
                    self.answer_dialogs()
                    t.join(timeout=0.2)
                    if not t.is_alive():
                        break
                    if i == 10:
                        self.note(f"{name}: still waiting; AT-SPI sees {self.describe_windows()}")
            self.counts[name] = self.counts.get(name, 0) + 1
            n += 1
            self.done = n
            # Questions from file operations come later, on their own.
            if n % 5 == 0:
                self.answer_dialogs()
            time.sleep(self.rnd.uniform(0, self.args.pause))
            if time.monotonic() - last_rss > 10:
                self.sample_rss()
                last_rss = time.monotonic()
            if time.monotonic() - last_report > 60:
                last_report = time.monotonic()
                rss = self.rss[-1][1] if self.rss else 0
                st = os.statvfs(os.path.expanduser("~"))
                free = st.f_bavail * st.f_frsize // (1 << 20)
                self.note(f"{n} actions, {len(self.windows())} windows, {self.tab_count()} tabs, {self.churned} file changes, {rss} MB, home {free} MB free")
        self.actions_done = n
        self.stop.set()
        # Quitting must work and be quick (once nothing waits for an answer,
        # which no one could give then).
        if self.alive() and not self.answer_dialogs(rounds=40, ending=True):
            self.problem("dialogs still open after 40 answers")
        if self.alive():
            self.sample_rss()
            t = time.monotonic()
            self.call(APP_PATH, "org.gtk.Actions", "Activate", self.GLib.Variant("(sava{sv})", ("quit", [], {})), timeout=10000)
            for _ in range(30):
                if not self.alive():
                    break
                time.sleep(1)
                self.answer_dialogs()
                if not self.args.no_a11y:
                    n = self.cancel_operations()
                    if n:
                        self.note(f"after quit: cancelled {n} file operation(s) still running")
                if self.args.backend == "x11":
                    names = subprocess.run(["sh", "-c", f"for w in $(xdotool search --onlyvisible --pid {self.pid} ''); do xdotool getwindowname $w; done"],
                                           capture_output=True, text=True).stdout.split("\n")
                    self.note(f"after quit: windows over D-Bus {self.windows()}, visible {[n for n in names if n]}")
            try:
                self.arkha.wait(timeout=1)
                self.quit_time = time.monotonic() - t
                # Under gdb, its message says how Arkha ended.
                self.log.flush()
                tail = open(os.path.join(self.dir, "arkha.log"), errors="replace").read()[-3000:]
                if self.args.heaptrack:
                    if self.arkha.returncode != 0:
                        self.problem(f"Arkha quit with status {self.arkha.returncode}")
                elif "exited normally" not in tail:
                    m = re.search(r"exited with code (\d+)", tail)
                    self.problem(f"Arkha quit with status {m.group(1) if m else 'unknown'}")
            except subprocess.TimeoutExpired:
                self.problem("Arkha didn't quit within 30 s of being told to")
                self.backtrace("quit")
                self.arkha.kill()

    def finish(self):
        for p in self.procs:
            p.terminate()
            try:
                p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                p.kill()
        warn = self.warnings()
        lat = sorted(self.latencies) or [0]
        lines = [
            f"Soak run: {self.args.backend}, seed {self.args.seed}, {self.args.minutes} min" + (" under heaptrack" if self.args.heaptrack else ""),
            f"Actions: {getattr(self, 'actions_done', 0)}, file changes by others: {self.churned}",
            f"Response: median {lat[len(lat) // 2] * 1000:.0f} ms, 99% {lat[int(len(lat) * 0.99)] * 1000:.0f} ms, slowest {lat[-1] * 1000:.0f} ms",
        ]
        if self.rss:
            start = [m for t, m in self.rss[: max(1, len(self.rss) // 5)]]
            endv = [m for t, m in self.rss[-max(1, len(self.rss) // 5):]]
            lines.append(f"Tabs at most: {self.most_tabs}")
            lines.append(f"Memory (RSS): first fifth {min(start)}–{max(start)} MB, last fifth {min(endv)}–{max(endv)} MB, highest {max(m for t, m in self.rss)} MB")
        c = getattr(self, "cycle_rss", [])
        if len(c) >= 20:
            # Growth per round after the first fifth (warming up: caches).
            xs = list(range(len(c) // 5, len(c)))
            ys = [c[i] for i in xs]
            mx, my = sum(xs) / len(xs), sum(ys) / len(ys)
            slope = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / max(1, sum((x - mx) ** 2 for x in xs))
            lines.append(f"Rounds: {len(c)}; memory after round {len(c) // 5}: {c[len(c) // 5]} MB, after the last: {c[-1]} MB; "
                         f"growth since: {slope * 1024:.0f} KB per round")
        if getattr(self, "restarts", 0):
            lines.append(f"Started again {self.restarts}× after its last window was closed")
        if hasattr(self, "quit_time"):
            lines.append(f"Quit took {self.quit_time:.1f} s")
        lines.append(f"Problems: {len(self.problems)}")
        lines += ["  " + p for p in self.problems]
        lines.append(f"Warnings in the log: {sum(warn.values())} ({len(warn)} kinds)")
        lines += [f"  {n}× {w}" for w, n in sorted(warn.items(), key=lambda x: -x[1])]
        bt = self.arkha_frames()
        if bt:
            lines.append("Where Arkha was stopped (its own frames, innermost first):")
            lines += ["  " + f for f in bt]
        lines.append("Most used actions: " + ", ".join(f"{k} {v}" for k, v in sorted(self.counts.items(), key=lambda x: -x[1])[:15]))
        report = "\n".join(lines)
        # The measurements, for charts.
        import json
        with open(os.path.join(self.dir, "data.json"), "w") as f:
            json.dump({
                "backend": self.args.backend, "seed": self.args.seed, "minutes": self.args.minutes,
                "mode": "cycle" if self.args.cycle else "random",
                "timeline": self.timeline,
                "rounds": getattr(self, "cycle_rss", []),
                "latencies_ms": [round(x * 1000, 1) for x in self.latencies],
                "actions": self.counts, "file_changes": self.churned,
                "problems": self.problems, "warnings": warn,
                "quit_s": round(getattr(self, "quit_time", -1), 2),
            }, f)
        with open(os.path.join(self.dir, "report.txt"), "w") as f:
            f.write(report + "\n")
        print("\n" + report)
        return 1 if self.problems or warn else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--backend", choices=["x11", "wayland"], default="x11")
    ap.add_argument("--minutes", type=float, default=10)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--language", default="", help="Arkha's language (a translation's code, e.g. de); the system's if not given")
    ap.add_argument("--name", help="added to the run's folder name (target/soak/<backend>-seed<N>-<name>), to keep runs of the same seed apart")
    ap.add_argument("--pause", type=float, default=0.15, help="longest pause between actions, in seconds")
    ap.add_argument("--binary", help="the arkha to run (default: one built with debug info in target/soak-build)")
    ap.add_argument("--heaptrack", action="store_true", help="run under heaptrack, to find memory that's never freed")
    ap.add_argument("--log-actions", action="store_true", help="write every action and dialog answer to actions.txt")
    ap.add_argument("--no-a11y", action="store_true", help="run Arkha without its accessibility bridge (no dialogs get answered)")
    ap.add_argument("--round", help='with --cycle: the round\'s actions, e.g. "new-tab,go-uri:tree,close-tab"')
    ap.add_argument("--cycle", action="store_true", help="repeat one round of actions instead of random ones, to see whether memory keeps growing")
    ap.add_argument("--fatal-criticals", action="store_true", help="stop Arkha at the first GLib/GTK critical warning, for its backtrace")
    ap.add_argument("--fatal-warnings", action="store_true", help="stop Arkha at the first warning (GLib's, GTK's, GVfs's…) or critical, for its backtrace")
    ap.add_argument("--inner", action="store_true", help=argparse.SUPPRESS)
    args = ap.parse_args()
    if not args.inner:
        sys.exit(outer(args))
    r = Run(args)
    try:
        r.go()
    finally:
        code = r.finish()
    sys.exit(code)


if __name__ == "__main__":
    main()

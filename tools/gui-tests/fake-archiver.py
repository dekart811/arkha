#!/usr/bin/env python3
"""A stand-in archive manager for the GUI tests: a small magenta window
(found in screenshots: Wayland gives no window positions) with an item to
drag out, in one of the two ways archive managers have.

    fake-archiver.py ark LOG   as KDE's Ark: the drag carries this program's
                               D-Bus connection name and the object path
                               /DndExtract/1, where org.kde.ark.DndExtract's
                               extractSelectedFilesTo(path) writes
                               from-ark.txt into that folder
    fake-archiver.py xds LOG   as Engrampa and the like (X11 only): X Direct
                               Save; the drop target writes where to save
                               into the window's XdndDirectSave0 property,
                               and from-xds.txt is written there

Each call is noted in LOG, one line each.
"""
import ctypes
import sys
from urllib.parse import unquote, urlparse

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, Gio, GLib, Gtk  # noqa: E402

MODE, LOG = sys.argv[1], sys.argv[2]
SERVICE = "application/x-kde-ark-dndextract-service"
PATH = "application/x-kde-ark-dndextract-path"
XDS = "XdndDirectSave0"
OBJECT = "/DndExtract/1"
IFACE = """<node><interface name="org.kde.ark.DndExtract">
  <method name="extractSelectedFilesTo"><arg name="path" type="s" direction="in"/></method>
</interface></node>"""


# gdk_property_change and gdk_property_get, which the Python bindings
# can't call, through ctypes.
gdk = ctypes.CDLL("libgdk-3.so.0")
glib = ctypes.CDLL("libglib-2.0.so.0")
gdk.gdk_atom_intern.restype = ctypes.c_void_p
gdk.gdk_atom_intern.argtypes = [ctypes.c_char_p, ctypes.c_int]
gdk.gdk_property_change.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
gdk.gdk_property_get.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int,
                                 ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_void_p)]
glib.g_free.argtypes = [ctypes.c_void_p]
pointer_of = ctypes.pythonapi.PyCapsule_GetPointer
pointer_of.restype, pointer_of.argtypes = ctypes.c_void_p, [ctypes.py_object, ctypes.c_char_p]


def atom(name):
    return gdk.gdk_atom_intern(name.encode(), 0)


def set_property(win, name, value):
    gdk.gdk_property_change(pointer_of(win.__gpointer__, None), atom(name), atom("text/plain"), 8, 0, value, len(value))


def get_property(win, name):
    kind, fmt, length, data = ctypes.c_void_p(), ctypes.c_int(), ctypes.c_int(), ctypes.c_void_p()
    if not gdk.gdk_property_get(pointer_of(win.__gpointer__, None), atom(name), atom("text/plain"), 0, 4096, 0,
                                ctypes.byref(kind), ctypes.byref(fmt), ctypes.byref(length), ctypes.byref(data)):
        return b""
    value = ctypes.string_at(data.value, length.value)
    glib.g_free(data)
    return value


def note(line):
    with open(LOG, "a") as f:
        f.write(line + "\n")


def save(folder, name, text):
    with open(f"{folder}/{name}", "w") as f:
        f.write(text)


bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
if MODE == "ark":
    def on_call(conn, sender, path, iface, method, params, invocation):
        dest = params.unpack()[0]
        note(f"extract {dest}")
        save(dest, "from-ark.txt", "from Ark\n")
        invocation.return_value(None)

    node = Gio.DBusNodeInfo.new_for_xml(IFACE)
    bus.register_object(OBJECT, node.interfaces[0], on_call, None, None)

win = Gtk.Window(title="Fake archive")
win.set_default_size(260, 160)
css = Gtk.CssProvider()
css.load_from_data(b"window, label { background: #ff00ff; color: black; }")
Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_USER)
box = Gtk.EventBox()
box.add(Gtk.Label(label="item.txt"))
win.add(box)
targets = [SERVICE, PATH] if MODE == "ark" else [XDS]
box.drag_source_set(Gdk.ModifierType.BUTTON1_MASK, [Gtk.TargetEntry.new(t, 0, 0) for t in targets], Gdk.DragAction.COPY | Gdk.DragAction.MOVE)


def on_begin(widget, ctx):
    note("drag")
    if MODE == "xds":
        # The name to save under, which the drop target reads.
        set_property(ctx.get_source_window(), XDS, b"from-xds.txt")


def on_get(widget, ctx, sel, info, time):
    target = sel.get_target().name()
    note(f"asked {target}")
    if target == SERVICE:
        sel.set(sel.get_target(), 8, bus.get_unique_name().encode())
    elif target == PATH:
        sel.set(sel.get_target(), 8, OBJECT.encode())
    elif target == XDS:
        # Where the drop target said to save: a file:// URI in the property.
        uri = get_property(ctx.get_source_window(), XDS).decode(errors="replace").rstrip("\0")
        note(f"save {uri}")
        path = unquote(urlparse(uri).path)
        try:
            with open(path, "w") as f:
                f.write("from XDS\n")
            sel.set(sel.get_target(), 8, b"S")
        except OSError:
            sel.set(sel.get_target(), 8, b"E")


box.connect("button-press-event", lambda *a: note("press"))
box.connect("drag-begin", on_begin)
box.connect("drag-data-get", on_get)
win.connect("destroy", Gtk.main_quit)
win.show_all()
note(f"ready {MODE} {bus.get_unique_name()}")
Gtk.main()

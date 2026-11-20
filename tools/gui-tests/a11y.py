#!/usr/bin/env python3
"""What accessibility (AT-SPI) shows of Arkha, for the GUI tests: works the
same on X11 and Wayland, where window titles can't be read otherwise.

    a11y.py windows     the titles of Arkha's windows, one per line
    a11y.py tabs        the tab names of its first window, in order
    a11y.py dialogs     its open dialogs' titles
    a11y.py headers     the column headers of its first window's list
    a11y.py cells       the texts of that list's cells, a line each
    a11y.py press NAME  presses the button, check box or radio button called NAME in its
                        dialogs or message boxes (as a screen reader would)
    a11y.py tab NAME    shows the dialog's tab called NAME
    a11y.py where TEXT  "x y", the middle of the shown button in its first
                        window whose name or tooltip starts with TEXT (to
                        click it with real input)
    a11y.py dialog-tabs the tab names of its first dialog, in order
    a11y.py buttons     its first dialog's (or message box's) buttons: "name|on"
                        or "name|off"
    a11y.py labels      its first dialog's (or message box's) texts, a line each
    a11y.py window-labels  the main windows' texts (status bar, side pane, …)
    a11y.py press-in TITLE TEXT  presses that button in its window TITLE
                        (* for any), as a screen reader would
    a11y.py labels-in TITLE  the texts in its window called TITLE, in order
    a11y.py focused-text  the text of the text field that has the focus
    a11y.py where-tab NAME  "x y", the middle of that dialog's tab NAME
    a11y.py focus ROLE  gives the dialog's first ROLE (e.g. text) the focus
    a11y.py listen S    keeps listening to focus events for S seconds, as
                        screen readers do (GTK then does more for them)
"""
import sys

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib  # noqa: E402


def arkha():
    desk = Atspi.get_desktop(0)
    for i in range(desk.get_child_count()):
        app = desk.get_child_at_index(i)
        if app and app.get_name() == "arkha":
            return app
    return None


def windows(app, roles=("frame",)):
    out = []
    for i in range(app.get_child_count()):
        w = app.get_child_at_index(i)
        if w and w.get_role_name() in roles:
            out.append(w)
    return out


def find(a, role, out, limit=4000, showing=True):
    """Everything of `role` under `a` (only shown ones if `showing`), depth first."""
    stack = [a]
    while stack and limit > 0:
        limit -= 1
        x = stack.pop()
        try:
            if x.get_role_name() == role and (not showing or x.get_state_set().contains(Atspi.StateType.SHOWING)):
                out.append(x)
            n = x.get_child_count()
            stack.extend(c for c in (x.get_child_at_index(k) for k in range(n - 1, -1, -1)) if c)
        except GLib.Error:
            pass
    return out


def main():
    Atspi.init()
    what = sys.argv[1] if len(sys.argv) > 1 else "windows"
    if what == "listen":
        listener = Atspi.EventListener.new(lambda e: None)
        for ev in ("object:active-descendant-changed", "object:state-changed", "focus:"):
            listener.register(ev)
        GLib.timeout_add_seconds(int(sys.argv[2]) if len(sys.argv) > 2 else 30, lambda: Atspi.event_quit())
        Atspi.event_main()
        return
    app = arkha()
    if not app:
        return
    if what == "windows":
        for w in windows(app):
            print(w.get_name())
    elif what == "tabs":
        ws = windows(app)
        if ws:
            # Tabs scrolled out of the tab bar too.
            for t in find(ws[0], "page tab", [], showing=False):
                print(t.get_name())
    elif what == "dialogs":
        for w in windows(app, ("dialog", "alert", "file chooser")):
            print(w.get_name())
    elif what == "headers":
        ws = windows(app)
        tables = find(ws[0], "tree table", []) + find(ws[0], "table", []) if ws else []
        if tables:
            t = tables[0]
            for i in range(Atspi.Table.get_n_columns(t)):
                h = Atspi.Table.get_column_header(t, i)
                print(h.get_name() if h else Atspi.Table.get_column_description(t, i))
    elif what == "cells":
        ws = windows(app)
        if ws:
            for t in find(ws[0], "table cell", []):
                name = t.get_name()
                if name:
                    print(name)
    elif what == "press":
        for d in windows(app, ("dialog", "alert", "file chooser")):
            for role in ("check box", "radio button", "button", "push button", "toggle button"):
                for b in find(d, role, []):
                    if b.get_name().replace("_", "") == sys.argv[2]:
                        b.get_action_iface().do_action(0)
                        return
        sys.exit(1)
    elif what == "tab":
        for d in windows(app, ("dialog",)):
            for tabs in find(d, "page tab list", []):
                for i in range(tabs.get_child_count()):
                    if tabs.get_child_at_index(i).get_name() == sys.argv[2]:
                        Atspi.Selection.select_child(tabs, i)
                        return
        sys.exit(1)
    elif what == "where":
        ws = windows(app)
        for role in ("button", "push button", "toggle button"):
            for b in find(ws[0], role, []) if ws else []:
                if (b.get_name() or "").startswith(sys.argv[2]) or (b.get_description() or "").startswith(sys.argv[2]):
                    e = Atspi.Component.get_extents(b, Atspi.CoordType.SCREEN)
                    print(e.x + e.width // 2, e.y + e.height // 2)
                    return
        sys.exit(1)
    elif what in ("dialog-tabs", "where-tab"):
        for d in windows(app, ("dialog",)):
            for t in find(d, "page tab", [], showing=False):
                if what == "dialog-tabs":
                    print(t.get_name())
                elif t.get_name() == sys.argv[2]:
                    e = Atspi.Component.get_extents(t, Atspi.CoordType.SCREEN)
                    print(e.x + e.width // 2, e.y + e.height // 2)
                    return
            if what == "dialog-tabs":
                return
        if what == "where-tab":
            sys.exit(1)
    elif what in ("buttons", "labels"):
        for d in windows(app, ("dialog", "alert")):
            roles = ("button", "push button") if what == "buttons" else ("label",)
            for role in roles:
                for x in find(d, role, []):
                    if what == "buttons":
                        on = x.get_state_set().contains(Atspi.StateType.SENSITIVE)
                        print(f"{x.get_name()}|{'on' if on else 'off'}")
                    elif x.get_name():
                        print(x.get_name())
            return
    elif what == "window-labels":
        # The labels shown in the main windows (status bar, side pane, …).
        for w in windows(app):
            for x in find(w, "label", []):
                if x.get_name():
                    print(x.get_name())
    elif what == "press-in":
        # Presses the button whose name or tooltip starts with sys.argv[3]
        # in the window called sys.argv[2] ("*": any), as a screen reader
        # would: no screen position needed (Wayland gives none).
        for w in windows(app):
            if sys.argv[2] not in ("*", w.get_name()):
                continue
            for role in ("button", "push button", "toggle button"):
                for b in find(w, role, []):
                    if (b.get_name() or "").startswith(sys.argv[3]) or (b.get_description() or "").startswith(sys.argv[3]):
                        b.get_action_iface().do_action(0)
                        return
        sys.exit(1)
    elif what == "labels-in":
        # The labels shown in the window called sys.argv[2], top to bottom.
        for w in windows(app):
            if w.get_name() == sys.argv[2]:
                for x in find(w, "label", []):
                    if x.get_name():
                        print(x.get_name())
    elif what == "focused-text":
        for w in windows(app):
            for x in find(w, "text", []):
                if x.get_state_set().contains(Atspi.StateType.FOCUSED):
                    print(Atspi.Text.get_text(x, 0, -1))
                    return
    elif what == "focus":
        for d in windows(app, ("dialog",)):
            for x in find(d, sys.argv[2], []):
                Atspi.Component.grab_focus(x)
                return
        sys.exit(1)


main()

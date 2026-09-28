#!/usr/bin/env python3
"""Publish two StatusNotifierItems and print every click Drifter routes back.

Run it on the machine Drifter runs on, from any shell that can see the session
bus. It needs python3-dbus and nothing else.
"""

import struct
import sys

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

ITEM = "org.kde.StatusNotifierItem"
WATCHER = "org.kde.StatusNotifierWatcher"
WATCHER_PATH = "/StatusNotifierWatcher"


def square(size, color):
    """An ARGB32 pixmap: a filled square with a hole in the middle."""
    a, r, g, b = color
    rows = []
    for y in range(size):
        for x in range(size):
            edge = x < 3 or y < 3 or x >= size - 3 or y >= size - 3
            middle = size // 4 <= x < size - size // 4 and size // 4 <= y < size - size // 4
            if edge or middle:
                rows.append(struct.pack("BBBB", a, r, g, b))
            else:
                rows.append(struct.pack("BBBB", 0, 0, 0, 0))
    return dbus.ByteArray(b"".join(rows))


class Item(dbus.service.Object):
    def __init__(self, bus, path, name, color):
        super().__init__(bus, path)
        self.name = name
        self.pixmap = square(24, color)

    @dbus.service.method("org.freedesktop.DBus.Properties",
                         in_signature="ss", out_signature="v")
    def Get(self, interface, prop):
        return self.GetAll(interface).get(prop, "")

    @dbus.service.method("org.freedesktop.DBus.Properties",
                         in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return {
            "Category": "ApplicationStatus",
            "Id": self.name,
            "Title": self.name,
            "Status": "Active",
            "IconName": "",
            "IconPixmap": dbus.Array(
                [dbus.Struct((dbus.Int32(24), dbus.Int32(24), self.pixmap),
                             signature=None)],
                signature="(iiay)",
            ),
            "ItemIsMenu": False,
        }

    @dbus.service.method(ITEM, in_signature="ii")
    def Activate(self, x, y):
        print(f"{self.name}: left click at {x} {y}", flush=True)

    @dbus.service.method(ITEM, in_signature="ii")
    def ContextMenu(self, x, y):
        print(f"{self.name}: right click at {x} {y}", flush=True)

    @dbus.service.method(ITEM, in_signature="ii")
    def SecondaryActivate(self, x, y):
        print(f"{self.name}: middle click at {x} {y}", flush=True)


def main():
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    name = dbus.service.BusName(f"org.drifter.TrayDemo{os_pid()}", bus)

    items = [
        Item(bus, "/StatusNotifierItem", "drifter-demo-a", (255, 90, 200, 255)),
        Item(bus, "/StatusNotifierItem2", "drifter-demo-b", (255, 255, 190, 90)),
    ]

    try:
        watcher = bus.get_object(WATCHER, WATCHER_PATH)
    except dbus.DBusException:
        print(f"no {WATCHER} on the bus. is drifter running?", file=sys.stderr)
        return 1

    register = watcher.get_dbus_method("RegisterStatusNotifierItem", WATCHER)
    for item in items:
        register(item._object_path)
        print(f"registered {item.name} at {item._object_path}", flush=True)

    print("waiting for clicks. ctrl-c to stop.", flush=True)
    GLib.MainLoop().run()
    return 0


def os_pid():
    import os
    return os.getpid()


if __name__ == "__main__":
    sys.exit(main())

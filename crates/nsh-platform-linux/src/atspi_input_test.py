"""Check how the test process's labelled input reads through AT-SPI."""

import ctypes
import os
import time

atspi = ctypes.CDLL("libatspi.so.0")
gobject = ctypes.CDLL("libgobject-2.0.so.0")
glib = ctypes.CDLL("libglib-2.0.so.0")
pointer = ctypes.c_void_p
integer = ctypes.c_int


def bind(library, name, result, arguments):
    function = getattr(library, name)
    function.restype = result
    function.argtypes = arguments
    return function


initialize = bind(atspi, "atspi_init", integer, [])
desktop = bind(atspi, "atspi_get_desktop", pointer, [integer])
child_count = bind(atspi, "atspi_accessible_get_child_count", integer, [pointer, pointer])
child_at = bind(atspi, "atspi_accessible_get_child_at_index", pointer, [pointer, integer, pointer])
process_id = bind(atspi, "atspi_accessible_get_process_id", ctypes.c_uint, [pointer, pointer])
get_name = bind(atspi, "atspi_accessible_get_name", pointer, [pointer, pointer])
get_description = bind(atspi, "atspi_accessible_get_description", pointer, [pointer, pointer])
get_role_name = bind(atspi, "atspi_accessible_get_role_name", pointer, [pointer, pointer])
get_states = bind(atspi, "atspi_accessible_get_state_set", pointer, [pointer])
contains = bind(atspi, "atspi_state_set_contains", integer, [pointer, integer])
unref = bind(gobject, "g_object_unref", None, [pointer])
free = bind(glib, "g_free", None, [pointer])

EDITABLE, FOCUSED, INVALID_ENTRY = 7, 12, 36


def string(function, node):
    value = function(node, None)
    if not value:
        return ""
    try:
        return ctypes.string_at(value).decode()
    finally:
        free(value)


def walk(node, found):
    found.append((string(get_role_name, node), string(get_name, node), node))
    count = child_count(node, None)
    for index in range(max(count, 0)):
        child = child_at(node, index, None)
        if child:
            walk(child, found)


assert initialize() == 0, "AT-SPI initialization failed"
root = desktop(0)
nodes = []
deadline = time.monotonic() + 3
while time.monotonic() < deadline:
    nodes = []
    for index in range(max(child_count(root, None), 0)):
        application = child_at(root, index, None)
        if application and process_id(application, None) == os.getppid():
            walk(application, nodes)
    if any(name == "Email" for _, name, _ in nodes):
        break
    time.sleep(0.02)

named = [(role, node) for role, name, node in nodes if name == "Email"]
roles = [role for role, _ in named]
print(f"AT-SPI input roles={roles}", flush=True)
assert len(named) == 1, f"one element reads the input's label, got {roles}"
role, node = named[0]
assert role in ("entry", "text"), f"the input is a text box, got {role}"
assert string(get_description, node) == "Work address"
states = get_states(node)
for state, what in [(EDITABLE, "editable"), (FOCUSED, "focused"), (INVALID_ENTRY, "invalid entry")]:
    assert contains(states, state), f"the input is not {what}"
unref(states)
print("AT-SPI input ok", flush=True)

# Read accessibility metadata only. Never read text, names or field values.
import json
import sys
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi
Atspi.set_timeout(150, 150)
queue = [Atspi.get_desktop(0)]
seen = 0
regions = "--regions" in sys.argv
result = [] if regions else None
while queue and seen < 2048:
    item = queue.pop()
    seen += 1
    try:
        states = item.get_state_set()
        role = item.get_role()
        if role in (Atspi.Role.FRAME, Atspi.Role.DIALOG, Atspi.Role.WINDOW) and not states.contains(Atspi.StateType.ACTIVE):
            continue
        if states.contains(Atspi.StateType.FOCUSED) or (regions and states.contains(Atspi.StateType.FOCUSABLE)):
            role = item.get_role()
            editable = (states.contains(Atspi.StateType.EDITABLE) or role == Atspi.Role.TERMINAL)
            if editable and (states.contains(Atspi.StateType.ENABLED) or states.contains(Atspi.StateType.SENSITIVE)):
                component = item.get_component_iface()
                rect = component.get_extents(Atspi.CoordType.SCREEN)
                relative = component.get_extents(Atspi.CoordType.WINDOW)
                # Some GTK/Wayland providers return a fabricated (0,0) screen origin.
                if rect.x == 0 and rect.y == 0 and (relative.x != 0 or relative.y != 0):
                    continue
                bounds = dict(x=rect.x, y=rect.y, width=rect.width, height=rect.height)
                if regions:
                    result.append(bounds)
                    if len(result) >= 32: break
                else:
                    result = bounds
                    break
        # Prune invisible subtrees, but retain desktop/application roots.
        if item.get_role() in (Atspi.Role.DESKTOP_FRAME, Atspi.Role.APPLICATION) or states.contains(Atspi.StateType.SHOWING):
            queue.extend(item.get_child_at_index(i) for i in range(min(item.get_child_count(), 256)))
    except Exception:
        pass
print(json.dumps(result), flush=True)

"""Everything that differs between Windows and macOS lives here."""
import ctypes
import getpass
import os
import subprocess
import sys
from pathlib import Path

WIN = sys.platform == "win32"
MAC = sys.platform == "darwin"

# colour Tk renders as see-through (and click-through)
KEY = "systemTransparent" if MAC else "#ff00fe"


def make_transparent(win):
    """Make a borderless Tk window's KEY-coloured background see-through."""
    if WIN:
        win.attributes("-transparentcolor", KEY)
    elif MAC:
        try:
            win.attributes("-transparent", True)
        except Exception:
            pass
    win.configure(bg=KEY)


def ui_scale(widget):
    """Pixels per 'design pixel'. Windows reports real DPI; macOS already works in points."""
    if MAC:
        return 1.0
    return widget.winfo_fpixels("1i") / 96.0


def dpi_aware():
    if WIN:
        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(1)
        except Exception:
            pass


# ------------------------------------------------------------------ screen work area
if WIN:
    from ctypes import wintypes

    class _MONITORINFO(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("rcMonitor", wintypes.RECT),
                    ("rcWork", wintypes.RECT), ("dwFlags", wintypes.DWORD)]

    _user32 = ctypes.windll.user32
    _user32.MonitorFromPoint.argtypes = [wintypes.POINT, wintypes.DWORD]
    _user32.MonitorFromPoint.restype = wintypes.HMONITOR
    _user32.GetMonitorInfoW.argtypes = [wintypes.HMONITOR, ctypes.POINTER(_MONITORINFO)]


class _CGRect(ctypes.Structure):
    _fields_ = [("x", ctypes.c_double), ("y", ctypes.c_double), ("w", ctypes.c_double), ("h", ctypes.c_double)]


_mac_cache = None


def _mac_screens():
    """[(frame, visibleFrame)] for every display, via the Objective-C runtime (no extra packages)."""
    objc = ctypes.cdll.LoadLibrary("/usr/lib/libobjc.A.dylib")
    ctypes.cdll.LoadLibrary("/System/Library/Frameworks/AppKit.framework/AppKit")
    objc.objc_getClass.restype = ctypes.c_void_p
    objc.objc_getClass.argtypes = [ctypes.c_char_p]
    objc.sel_registerName.restype = ctypes.c_void_p
    objc.sel_registerName.argtypes = [ctypes.c_char_p]
    import platform
    plain = objc.objc_msgSend
    stret = getattr(objc, "objc_msgSend_stret", None) if platform.machine() == "x86_64" else None

    def msg(obj, sel, restype=ctypes.c_void_p, *args, argtypes=()):
        send = stret if (stret is not None and restype is _CGRect) else plain
        send.restype = restype
        send.argtypes = [ctypes.c_void_p, ctypes.c_void_p, *argtypes]
        return send(obj, objc.sel_registerName(sel), *args)

    screens = msg(objc.objc_getClass(b"NSScreen"), b"screens")
    n = msg(screens, b"count", ctypes.c_ulong)
    out = []
    for i in range(n):
        s = msg(screens, b"objectAtIndex:", ctypes.c_void_p, i, argtypes=(ctypes.c_ulong,))
        out.append((msg(s, b"frame", _CGRect), msg(s, b"visibleFrame", _CGRect)))
    return out


def work_area_at(root, x, y):
    """Usable desktop (l, t, r, b) of the screen nearest x, y - i.e. without taskbar / Dock / menu bar."""
    if WIN:
        try:
            hmon = _user32.MonitorFromPoint(wintypes.POINT(int(x), int(y)), 2)
            mi = _MONITORINFO()
            mi.cbSize = ctypes.sizeof(mi)
            if _user32.GetMonitorInfoW(hmon, ctypes.byref(mi)):
                r = mi.rcWork
                return r.left, r.top, r.right, r.bottom
        except Exception:
            pass
    elif MAC:
        global _mac_cache
        try:
            if _mac_cache is None:
                _mac_cache = _mac_screens()
            main_h = _mac_cache[0][0].h       # Cocoa's y axis starts at the bottom of the main screen
            best = None
            for frame, vis in _mac_cache:
                l, t = vis.x, main_h - (vis.y + vis.h)
                r, b = l + vis.w, t + vis.h
                d = (0 if l <= x <= r else min(abs(x - l), abs(x - r))) + \
                    (0 if t <= y <= b else min(abs(y - t), abs(y - b)))
                if best is None or d < best[0]:
                    best = (d, (int(l), int(t), int(r), int(b)))
            if best:
                return best[1]
        except Exception:
            pass
    return 0, 0, root.winfo_screenwidth(), root.winfo_screenheight() - 40


# ------------------------------------------------------------------ one copy at a time
_LOCK = None


def single_instance(name):
    global _LOCK
    if WIN:
        k32 = ctypes.WinDLL("kernel32", use_last_error=True)
        k32.CreateMutexW.restype = ctypes.c_void_p
        _LOCK = k32.CreateMutexW(None, False, "Local\\" + name)
        return ctypes.get_last_error() != 183  # ERROR_ALREADY_EXISTS
    import fcntl
    path = Path.home() / "Library" / "Application Support" / "Blip" if MAC else Path.home() / ".blip"
    path.mkdir(parents=True, exist_ok=True)
    _LOCK = open(path / f"{name}.lock", "w")
    try:
        fcntl.flock(_LOCK, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return True
    except OSError:
        return False


# ------------------------------------------------------------------ start at login
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
RUN_NAME = "BlipPet"
AGENT = Path.home() / "Library" / "LaunchAgents" / "com.safder2000.blip.plist"


def app_bundle():
    """Path of Blip.app when running as the packaged Mac app."""
    if MAC and getattr(sys, "frozen", False):
        p = Path(sys.executable).resolve()
        for parent in p.parents:
            if parent.suffix == ".app":
                return parent
    return None


def startup_enabled():
    if WIN:
        try:
            import winreg
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as k:
                winreg.QueryValueEx(k, RUN_NAME)
            return True
        except OSError:
            return False
    if MAC:
        return AGENT.exists()
    return False


def set_startup(on, command_args):
    """command_args: argv list that starts Blip."""
    if WIN:
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as k:
            if on:
                winreg.SetValueEx(k, RUN_NAME, 0, winreg.REG_SZ, " ".join(f'"{a}"' for a in command_args))
            else:
                try:
                    winreg.DeleteValue(k, RUN_NAME)
                except OSError:
                    pass
    elif MAC:
        if on:
            bundle = app_bundle()
            args = ["/usr/bin/open", str(bundle)] if bundle else command_args
            items = "".join(f"<string>{a}</string>" for a in args)
            AGENT.parent.mkdir(parents=True, exist_ok=True)
            AGENT.write_text(
                '<?xml version="1.0" encoding="UTF-8"?>\n'
                '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" '
                '"http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n'
                '<plist version="1.0"><dict>'
                '<key>Label</key><string>com.safder2000.blip</string>'
                f'<key>ProgramArguments</key><array>{items}</array>'
                '<key>RunAtLoad</key><true/></dict></plist>\n', encoding="utf-8")
        else:
            try:
                AGENT.unlink()
            except OSError:
                pass


# ------------------------------------------------------------------ misc
def full_name():
    """The signed-in person's display name, e.g. 'Safder Ali', or ''."""
    try:
        if WIN:
            fn = ctypes.windll.secur32.GetUserNameExW
            size = ctypes.c_ulong(0)
            fn(3, None, ctypes.byref(size))          # NameDisplay
            if size.value:
                buf = ctypes.create_unicode_buffer(size.value + 1)
                if fn(3, buf, ctypes.byref(size)):
                    return buf.value.strip()
        elif MAC:
            return subprocess.run(["id", "-F"], capture_output=True, text=True, timeout=3).stdout.strip()
    except Exception:
        pass
    return ""


def user_name():
    try:
        return getpass.getuser().split("\\")[-1]
    except Exception:
        return ""


def open_path(path):
    path = str(path)
    if WIN:
        os.startfile(path)
    elif MAC:
        subprocess.Popen(["open", path])
    else:
        subprocess.Popen(["xdg-open", path])


def system_dark():
    """True when the OS is set to dark mode for apps."""
    try:
        if WIN:
            import winreg
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER,
                                r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize") as k:
                return winreg.QueryValueEx(k, "AppsUseLightTheme")[0] == 0
        if MAC:
            r = subprocess.run(["defaults", "read", "-g", "AppleInterfaceStyle"], capture_output=True, text=True,
                               timeout=3)
            return "dark" in r.stdout.lower()
    except Exception:
        pass
    return True

"""Loads the randomizer module built for this computer.

Each system's module sits in lib/<system>-<machine>/, so one apworld works on Windows, macOS, and
Linux. The modules use Python's stable ABI, so one build covers Python 3.11 and newer.
"""

import importlib.machinery
import importlib.util
import os
import platform
import sys


def _system_tag() -> str:
    return f"{sys.platform}-{platform.machine().lower()}"


def _load():
    folder = os.path.join(os.path.dirname(__file__), "lib", _system_tag())
    if os.path.isdir(folder):
        for entry in sorted(os.listdir(folder)):
            if entry.startswith("albw3dsrandomizer") and entry.endswith(tuple(importlib.machinery.EXTENSION_SUFFIXES)):
                path = os.path.join(folder, entry)
                spec = importlib.util.spec_from_file_location(f"{__name__}.albw3dsrandomizer", path)
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)
                return module
    available = os.listdir(os.path.join(os.path.dirname(__file__), "lib")) if os.path.isdir(os.path.join(os.path.dirname(__file__), "lib")) else []
    raise ImportError(
        f"The A Link Between Worlds - 3DS apworld has no randomizer for {_system_tag()}. "
        f"It has: {', '.join(sorted(available)) or 'none'}."
    )


albw3dsrandomizer = _load()
sys.modules[f"{__name__}.albw3dsrandomizer"] = albw3dsrandomizer
from .albw3dsrandomizer import *  # noqa: E402,F401,F403

__all__ = albw3dsrandomizer.__all__

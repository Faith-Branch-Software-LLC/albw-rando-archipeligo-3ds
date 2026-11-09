import atexit
import random
import os
import shutil
import sys
import tempfile
import zipfile
from Utils import local_path

albw_base_id = 6242624000

tmp_path = ""

def get_temp_path():
    global tmp_path
    return tmp_path

def setup_lib():
    global tmp_path

    # clean up any files from old versions of the apworld
    for path in [local_path("."), local_path("lib")]:
        if os.path.exists(path):
            for entry in os.scandir(path):
                if entry.name.startswith("albw3dsrandomizer"):
                    fullpath = os.path.join(path, entry.name)
                    if entry.is_dir():
                        shutil.rmtree(fullpath)
                    else:
                        os.remove(fullpath)

    # clean up old temp directories
    for directory in os.listdir(tempfile.gettempdir()):
        if directory.startswith("albw3dsrandomizer"):
            path = os.path.join(tempfile.gettempdir(), directory)
            if os.path.exists(os.path.join(path, "cleanup-tag")):
                try:
                    shutil.rmtree(path)
                except:
                    pass

    apworld_path = os.path.dirname(os.path.dirname(__file__))
    if apworld_path.endswith(".apworld"):
        with zipfile.ZipFile(apworld_path, "r") as apworld:
            tmp_path = tempfile.mkdtemp(prefix="albw3dsrandomizer")
            atexit.register(cleanup_lib)
            try:
                randomizer_path = os.path.join(tmp_path, "albw3dsrandomizer")
                if not os.path.exists(randomizer_path):
                    os.mkdir(randomizer_path)
                # Keeps the folders under albw3dsrandomizer/, which hold one module per system.
                prefix = "albw_3ds/albw3dsrandomizer/"
                for info in apworld.infolist():
                    if not info.is_dir() and info.filename.startswith(prefix):
                        info.filename = info.filename[len(prefix):]
                        apworld.extract(info, randomizer_path)
            except Exception as e:
                from tkinter.messagebox import showerror
                showerror(message=str(e))
            if not tmp_path in sys.path:
                sys.path.append(tmp_path)
    else:
        # The world's own folder holds the albw3dsrandomizer package. It goes last on the path, so
        # Archipelago's own modules keep their names.
        path = os.path.dirname(__file__)
        if not path in sys.path:
            sys.path.append(path)

def cleanup_lib():
    global tmp_path

    if os.path.exists(tmp_path):
        try:
            shutil.rmtree(tmp_path)
        except Exception:
            with open(os.path.join(tmp_path, "cleanup-tag"), "w") as file:
                pass

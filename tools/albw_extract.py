#!/usr/bin/env python3
"""Pulls one file out of A Link Between Worlds' game data, for modding.

Reads a decrypted USA dump (.cci or .3ds) or an extracted RomFS folder. Archives (.szs) are
Yaz0-compressed SARC files whose file names are stored only as hashes, so a name inside an archive
is found by hashing it.

Examples:
  python3 tools/albw_extract.py game.cci Archive/ActorCommon.szs World/Actor/link.bch link.bch
  python3 tools/albw_extract.py romfs/ Archive/DungeonCastle7.szs World/Actor/Zelda.bch Zelda.bch
  python3 tools/albw_extract.py game.cci World/PlayerItem/FlatLinkLv1.bch FlatLinkLv1.bch
  python3 tools/albw_extract.py game.cci Archive/ActorCommon.szs --list
  python3 tools/albw_extract.py game.cci Archive/ActorCommon.szs --unpack ActorCommon.sarc

The output is the user's own game data. Never share it or commit it.
"""

import os
import struct
import sys


def romfs_files(dump_path):
    """Maps each RomFS path to (offset, size) inside a decrypted .cci or .3ds dump."""
    dump = open(dump_path, "rb")

    def read(offset, size):
        dump.seek(offset)
        return dump.read(size)

    if read(0x100, 4) != b"NCSD":
        sys.exit("This is not a .cci or .3ds dump.")
    ncch = struct.unpack("<I", read(0x120, 4))[0] * 0x200
    if read(ncch + 0x100, 4) != b"NCCH":
        sys.exit("The dump's first partition is not readable.")
    # Byte 7 of the NCCH flags is set when the partition is not encrypted.
    if not read(ncch + 0x18F, 1)[0] & 0x4:
        sys.exit("The dump is encrypted. Decrypt it first, for example with GodMode9.")
    romfs = ncch + struct.unpack("<I", read(ncch + 0x1B0, 4))[0] * 0x200
    if read(romfs, 4) != b"IVFC":
        sys.exit("The dump has no RomFS.")
    master_size = struct.unpack("<I", read(romfs + 0x08, 4))[0]
    block = 1 << struct.unpack("<I", read(romfs + 0x4C, 4))[0]
    level3 = romfs + ((0x60 + master_size + block - 1) // block) * block
    header = struct.unpack("<10I", read(level3, 40))
    dirs, files, data = level3 + header[3], level3 + header[7], level3 + header[9]

    found = {}

    def walk(entry, path):
        sibling, child, first_file, name_size = struct.unpack("<4x3I4xI", read(dirs + entry, 24))
        name = read(dirs + entry + 24, name_size).decode("utf-16-le")
        here = path + name + "/" if entry else ""
        file_entry = first_file
        while file_entry != 0xFFFFFFFF:
            next_file, offset, size, file_name_size = struct.unpack("<4xIQQ4xI", read(files + file_entry, 32))
            file_name = read(files + file_entry + 32, file_name_size).decode("utf-16-le")
            found[here + file_name] = (data + offset, size)
            file_entry = next_file
        while child != 0xFFFFFFFF:
            walk(child, here)
            child = struct.unpack("<I", read(dirs + child + 4, 4))[0]

    walk(0, "")
    return dump, found


def read_game_file(source, path):
    """The bytes of one RomFS file, from a dump or an extracted RomFS folder."""
    if os.path.isdir(source):
        with open(os.path.join(source, path), "rb") as file:
            return file.read()
    dump, files = romfs_files(source)
    if path not in files:
        sys.exit(f"{path} is not in the RomFS.")
    offset, size = files[path]
    dump.seek(offset)
    return dump.read(size)


def yaz0(data):
    """Decompresses Yaz0. Data that is not compressed comes back as it is."""
    if data[:4] != b"Yaz0":
        return data
    size = struct.unpack(">I", data[4:8])[0]
    out = bytearray()
    at = 16
    while len(out) < size:
        code = data[at]
        at += 1
        for bit in range(7, -1, -1):
            if len(out) >= size:
                break
            if code & (1 << bit):
                out.append(data[at])
                at += 1
            else:
                first, second = data[at], data[at + 1]
                at += 2
                distance = ((first & 0xF) << 8 | second) + 1
                if first >> 4:
                    count = (first >> 4) + 2
                else:
                    count = data[at] + 0x12
                    at += 1
                start = len(out) - distance
                for index in range(count):
                    out.append(out[start + index])
    return bytes(out)


def name_hash(name, multiplier):
    value = 0
    for byte in name.encode():
        value = (value * multiplier + byte) & 0xFFFFFFFF
    return value


def sarc_entries(data):
    """Each file in a SARC archive as (hash, name or None, bytes)."""
    if data[:4] != b"SARC":
        sys.exit("That archive is not a SARC file.")
    order = "<" if data[6:8] == b"\xff\xfe" else ">"
    header_size = struct.unpack(order + "H", data[4:6])[0]
    data_start = struct.unpack(order + "I", data[12:16])[0]
    sfat = header_size
    count, multiplier = struct.unpack(order + "HI", data[sfat + 6:sfat + 12])
    names = sfat + 12 + count * 16 + 8
    entries = []
    for index in range(count):
        entry = sfat + 12 + index * 16
        hashed, attributes, start, end = struct.unpack(order + "4I", data[entry:entry + 16])
        name = None
        if attributes & 0x01000000:
            at = names + (attributes & 0xFFFF) * 4
            name = data[at:data.index(b"\0", at)].decode()
        entries.append((hashed, name, data[data_start + start:data_start + end]))
    return entries, multiplier


def main():
    if len(sys.argv) < 4:
        sys.exit(__doc__)
    source, path, inner = sys.argv[1], sys.argv[2], sys.argv[3]
    data = read_game_file(source, path)

    if not path.endswith((".szs", ".arc", ".sarc")):
        with open(inner, "wb") as out:
            out.write(data)
        print(f"Saved {path} ({len(data)} bytes) to {inner}")
        return

    archive = yaz0(data)
    if inner == "--unpack":
        with open(sys.argv[4], "wb") as out:
            out.write(archive)
        print(f"Saved {path}, decompressed ({len(archive)} bytes), to {sys.argv[4]}")
        return
    entries, multiplier = sarc_entries(archive)
    if inner == "--list":
        for hashed, name, body in entries:
            print(f"{name or f'hash:{hashed:08x}'}\t{len(body)}\t{body[:4]!r}")
        return
    wanted = name_hash(inner, multiplier)
    for hashed, name, body in entries:
        if name == inner or hashed == wanted:
            with open(sys.argv[4], "wb") as out:
                out.write(body)
            print(f"Saved {inner} ({len(body)} bytes) to {sys.argv[4]}")
            return
    sys.exit(f"{inner} is not in {path}.")


if __name__ == "__main__":
    main()

"""Writes crates/ap-patch/src/tables.rs from the apworld's Python files.

The 3DS app rebuilds patches from `patch_info.json`, which names items and locations the way the
apworld does. This copies the apworld's names, item meanings, location codes, and hint character
set into Rust, so the two cannot drift. Run it after changing Items.py, Locations.py, or Hints.py:

    python3 apworld/gen_tables.py
"""

import ast
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORLD = os.path.join(ROOT, "apworld", "albw_3ds")
OUT = os.path.join(ROOT, "crates", "ap-patch", "src", "tables.rs")


def parse(name):
    with open(os.path.join(WORLD, name), encoding="utf-8") as file:
        return ast.parse(file.read())


def rust_string(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def progress_of(call):
    """The first thing an item gives, as Rust code."""
    func = call.func.id
    if func == "goal":
        return f"Progress::Goal(Goal::{call.args[1].attr})"
    if func == "vane":
        return f"Progress::Vane(Vane::{call.args[1].attr})"
    assert func == "ItemData", func
    first = call.args[4].elts[0]
    kind = first.func.id
    member = first.args[0].attr
    return {
        "new_item": f"Progress::Item(Item::{member})",
        "new_goal": f"Progress::Goal(Goal::{member})",
        "new_vane": f"Progress::Vane(Vane::{member})",
    }[kind]


def items():
    tree = parse("Items.py")
    items_class = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "Items")
    found = []
    for node in items_class.body:
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Call):
            call = node.value
            name = call.args[0].value if call.func.id in ("goal", "vane") else call.args[1].value
            found.append((name, progress_of(call)))
    return found


def locations():
    found = []
    for node in ast.walk(parse("Locations.py")):
        if isinstance(node, ast.Call) and getattr(node.func, "id", None) == "LocationData":
            code = node.args[0].value
            found.append((node.args[3].value, code))
    return found


def charset():
    for node in parse("Hints.py").body:
        if isinstance(node, ast.Assign) and node.targets[0].id == "charset":
            return ast.literal_eval(node.value)
    raise RuntimeError("Hints.py has no charset")


def main():
    lines = [
        "// Written by apworld/gen_tables.py from the apworld's Python files. Do not edit by hand.",
        "",
        "use randomizer::filler::filler_item::{Goal, Item, Vane};",
        "",
        "/// What an apworld item gives the randomizer: its first progressive step.",
        "#[derive(Clone, Copy, Debug)]",
        "pub enum Progress {",
        "    Item(Item),",
        "    Goal(Goal),",
        "    Vane(Vane),",
        "}",
        "",
        "/// Each apworld item name and what it gives.",
        "pub const ITEMS: &[(&str, Progress)] = &[",
    ]
    lines += [f"    ({rust_string(name)}, {progress}),"
              for name, progress in items()]
    lines += [
        "];",
        "",
        "/// Each apworld location name and its code, which events lack.",
        "pub const LOCATIONS: &[(&str, Option<u16>)] = &[",
    ]
    lines += [f"    ({rust_string(name)}, {'None' if code is None else f'Some({code})'}),"
              for name, code in locations()]
    lines += [
        "];",
        "",
        "/// Characters the game's font can show. The apworld turns others into '?'.",
        f"pub const CHARSET: &str = {rust_string(charset())};",
        "",
    ]
    with open(OUT, "w", encoding="utf-8") as file:
        file.write("\n".join(lines))
    print(f"Wrote {OUT}")


main()

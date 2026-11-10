"""Writes crates/ap-patch/src/tables.rs and crates/ui-core/src/ap_option_table.rs from the
apworld's Python files.

The 3DS app rebuilds patches from `patch_info.json`, which names items and locations the way the
apworld does. This copies the apworld's names, item meanings, location codes, and hint character
set into Rust, so the two cannot drift. It also copies every option in Options.py, which the app's
Archipelago Options pages show. Run it after changing Items.py, Locations.py, Hints.py, or
Options.py:

    python3 apworld/gen_tables.py
"""

import ast
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORLD = os.path.join(ROOT, "apworld", "albw_3ds")
OUT = os.path.join(ROOT, "crates", "ap-patch", "src", "tables.rs")
OPTIONS_OUT = os.path.join(ROOT, "crates", "ui-core", "src", "ap_option_table.rs")

# The pages the app shows options on, and which options go on each.
CATEGORIES = [
    ("Multiworld", ["progression_balancing", "accessibility", "death_link"]),
    ("Goal and Logic", ["logic_mode", "lorule_castle_requirement", "pedestal_requirement", "trials_required",
                        "open_trials_door", "randomize_dungeon_prizes", "progression_bosses", "no_progression_enemies",
                        "lamp_and_net_as_weapons", "dark_rooms_lampless", "swordless_mode", "assured_weapon"]),
    ("Items", ["nice_items", "super_items", "shuffle_maiamai_rewards", "maiamai_limit", "maiamai_mayhem",
               "bow_of_light_in_castle", "purple_potion_bottles", "minigames_excluded"]),
    ("Start and Shop", ["start_with_merge", "start_with_pouch", "bell_in_shop", "sword_in_shop", "boots_in_shop"]),
    ("Dungeons", ["small_keys", "big_keys", "compasses", "key_rings", "chest_size_matches_contents"]),
    ("World", ["initial_crack_state", "crack_shuffle", "weather_vanes", "skip_big_bomb_flower",
               "treacherous_tower_floors", "hint_ghosts"]),
    ("Looks", ["night_mode", "change_freestanding_models"]),
]

# Options every Archipelago game has, from Archipelago's own Options.py.
COMMON = {
    "progression_balancing": ("Progression Balancing", "Range", (0, 99, 50),
        "How hard the generator tries to give you progression early. 0 turns it off, 50 is normal, and 99 "
        "is as early as it can.", []),
    "accessibility": ("Accessibility", "Choice", 0,
        "Whether every location must be reachable, or only enough to finish.",
        [("full", "Every location can be reached."),
         ("minimal", "Only what you need to finish can be reached.")]),
    "death_link": ("Death Link", "Toggle", 0,
        "When you die, everyone who turned on Death Link dies. The reverse is true too.", []),
}

# Short names for values that do not fit a pill.
SHORT = {
    "adv_glitched": "Adv. Glitched",
    "cross_world_pairs": "Cross World",
    "any_world_pairs": "Any World",
    "mirrored_cross_world_pairs": "Mirrored Cross",
    "mirrored_any_world_pairs": "Mirrored Any",
    "own_dungeons": "Own Dungeon",
    "startwith": "Start With",
    "remove": "Removed",
}

# How far one press moves a number.
STEPS = {"maiamai_limit": 5, "progression_balancing": 5}


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


def option_classes():
    """Each option class in Options.py: its kind, numbers, docstring, and display name."""
    found = {}
    for node in parse("Options.py").body:
        if not isinstance(node, ast.ClassDef) or not node.bases:
            continue
        base = getattr(node.bases[0], "id", None)
        if base not in ("Toggle", "DefaultOnToggle", "Choice", "Range"):
            continue
        values = {}
        for item in node.body:
            if isinstance(item, ast.Assign) and isinstance(item.targets[0], ast.Name):
                values[item.targets[0].id] = ast.literal_eval(item.value)
        found[node.name] = (base, values, ast.get_docstring(node) or "")
    return found


def option_fields():
    """The ALBWOptions fields in order, as (key, class name)."""
    for node in parse("Options.py").body:
        if isinstance(node, ast.ClassDef) and node.name == "ALBWOptions":
            return [(item.target.id, item.annotation.id) for item in node.body
                    if isinstance(item, ast.AnnAssign)]
    raise RuntimeError("Options.py has no ALBWOptions")


def split_doc(doc, keys):
    """The docstring's general text, and the help for each value it describes."""
    general, per_value = [], {}
    for line in doc.splitlines():
        line = line.strip()
        key, _, rest = line.partition(":")
        if key in keys and rest:
            per_value[key] = rest.strip()
        elif line:
            general.append(line)
    return " ".join(general), per_value


def describe_options():
    classes = option_classes()
    described = dict(COMMON)
    for key, class_name in option_fields():
        if class_name not in classes:
            continue  # DeathLink and StartInventoryPool come from Archipelago.
        base, values, doc = classes[class_name]
        label = values.get("display_name", key)
        if base in ("Toggle", "DefaultOnToggle"):
            general, _ = split_doc(doc, ())
            default = 1 if base == "DefaultOnToggle" or values.get("default") in (1, True) else 0
            described[key] = (label, "Toggle", default, general, [])
        elif base == "Choice":
            choices = sorted((number, name[len("option_"):]) for name, number in values.items()
                             if name.startswith("option_"))
            general, per_value = split_doc(doc, [name for _, name in choices])
            default_number = values.get("default", choices[0][0])
            default = [number for number, _ in choices].index(default_number)
            described[key] = (label, "Choice", default, general, [(name, per_value.get(name, "")) for _, name in choices])
        else:
            general, _ = split_doc(doc, ())
            low, high = values["range_start"], values["range_end"]
            described[key] = (label, "Range", (low, high, values.get("default", low)), general, [])
    listed = [key for _, keys in CATEGORIES for key in keys]
    missing = set(described) - set(listed)
    unknown = set(listed) - set(described)
    assert not missing and not unknown, f"Put these options in CATEGORIES: {missing}; unknown: {unknown}"
    return described


def write_options():
    described = describe_options()
    lines = [
        "// Written by apworld/gen_tables.py from the apworld's Options.py. Do not edit by hand.",
        "",
        "use crate::ap_yaml::{ApChoice, ApKind, ApOptionDef};",
        "",
        "/// The pages the Archipelago options are shown on.",
        "pub const AP_CATEGORIES: &[&str] = &[" + ", ".join(rust_string(name) for name, _ in CATEGORIES) + "];",
        "",
        "/// Every option in the apworld, in page order.",
        "pub const AP_OPTIONS: &[ApOptionDef] = &[",
    ]
    for category, (_, keys) in enumerate(CATEGORIES):
        for key in keys:
            label, kind, default, general, choices = described[key]
            if kind == "Toggle":
                kind_code, default_code = "ApKind::Toggle", default
            elif kind == "Choice":
                parts = ", ".join(
                    f"ApChoice {{ key: {rust_string(name)}, label: {rust_string(SHORT.get(name, name.replace('_', ' ').title()))}, "
                    f"help: {rust_string(text)} }}" for name, text in choices)
                kind_code, default_code = f"ApKind::Choice(&[{parts}])", default
            else:
                low, high, value = default
                kind_code = f"ApKind::Range {{ min: {low}, max: {high}, step: {STEPS.get(key, 1)} }}"
                default_code = value
            lines.append(
                f"    ApOptionDef {{ key: {rust_string(key)}, label: {rust_string(label)}, category: {category}, "
                f"help: {rust_string(general)}, kind: {kind_code}, default: {default_code} }},")
    lines += ["];", ""]
    with open(OPTIONS_OUT, "w", encoding="utf-8") as file:
        file.write("\n".join(lines))
    print(f"Wrote {OPTIONS_OUT}")


main()
write_options()

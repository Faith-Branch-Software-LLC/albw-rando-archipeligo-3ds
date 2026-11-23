// The seed's map file and the logic that runs over it (map_logic.c).
#pragma once

#include <stdbool.h>
#include <stdint.h>

typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;

#define MAP_VERSION 2
#define RULE_TRUE 0xFFFF
#define RULE_FALSE 0xFFFE
#define ITEM_NONE 0xFFFF
#define FLAG_NONE 0xFE   // the game keeps no flag the plugin knows for this check
#define FLAG_EVENT 0xFF  // the flag is one of the game's event flags, not a course's
#define MAP_NONE 0xFF    // the check has no place on the map

enum
{
    STATUS_OUT_OF_LOGIC = 0,
    STATUS_IN_LOGIC = 1,
    STATUS_CHECKED = 2,
    STATUS_GLITCHED = 3, // reachable only with glitches beyond the seed's logic
};

enum
{
    KIND_ITEM = 0,
    KIND_MAIAMAI = 1, // a Maiamai spot still holding its Maiamai, left off the map
    KIND_HIDDEN = 2,
};

typedef struct
{
    u16 rule, item, quest, flag, glitchRule;
    u8 flagCourse, map, x, y, course, stage, kind;
    const char *name;
} MapCheck;

typedef struct
{
    u8 map, x, y, course, stage;
} MapPlace;

typedef struct
{
    MapPlace from, to;
} MapCrack;

typedef struct
{
    u32 itemCount, groupCount, varCount, nodeCount, locationCount, edgeCount, checkCount, crackCount, start;
    u16 specialGroups[5]; // heart containers, heart pieces, purple, silver and gold rupees
    const u8 *groups, *groupItems, *vars, *nodes, *locations, *edges, *checks, *cracks;
    const char *names;
    // Working memory.
    u8 *owned, *answers, *reached, *inLogic, *inGlitched;
    u16 *queue;
} MapData;

int map_parse(MapData *map, const u8 *data, u32 size);
MapCheck map_check(const MapData *map, u32 index);
MapCrack map_crack(const MapData *map, u32 index);
// `checked` holds 1 for each check the game marks as done. Fills `status` with a STATUS_ value
// for each check.
void map_evaluate(MapData *map, const u8 *checked, u8 *status);

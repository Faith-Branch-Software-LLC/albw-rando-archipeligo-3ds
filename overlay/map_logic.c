// Reads the seed's map file and works out which checks are in logic. Plain C with no 3DS calls,
// so the tests can build it on a computer and compare it with the randomizer's own logic.
//
// The file is made by the randomizer's tracker module (tracker/mod.rs), which documents the layout.

#include "map_logic.h"

#include <stdlib.h>
#include <string.h>

// Each edge: destination, rule, glitched rule. Each check: see map_check.
#define EDGE_SIZE 6
#define CHECK_SIZE 22

static u32 word(const u8 *at)
{
    return at[0] | at[1] << 8 | at[2] << 16 | (u32)at[3] << 24;
}

static u16 half(const u8 *at)
{
    return at[0] | at[1] << 8;
}

int map_parse(MapData *map, const u8 *data, u32 size)
{
    memset(map, 0, sizeof(*map));
    if (size < 8 + 12 * 4 + 12 || memcmp(data, "ALBWMAP", 8) != 0 || word(data + 8) != MAP_VERSION)
        return -1;
    const u8 *at = data + 12;
    map->itemCount = word(at);
    map->groupCount = word(at + 4);
    u32 groupItems = word(at + 8);
    map->varCount = word(at + 12);
    map->nodeCount = word(at + 16);
    map->locationCount = word(at + 20);
    map->edgeCount = word(at + 24);
    map->checkCount = word(at + 28);
    map->crackCount = word(at + 32);
    u32 namesSize = word(at + 36);
    map->start = word(at + 40);
    at += 44;
    for (int i = 0; i < 5; i++)
        map->specialGroups[i] = half(at + i * 2);
    at += 12;

    map->groups = at;
    at += map->groupCount * 4;
    map->groupItems = at;
    at += groupItems * 2;
    map->vars = at;
    at += map->varCount * 6;
    map->nodes = at;
    at += map->nodeCount * 6;
    map->locations = at;
    at += map->locationCount * 8;
    map->edges = at;
    at += map->edgeCount * EDGE_SIZE;
    map->checks = at;
    at += map->checkCount * CHECK_SIZE;
    map->cracks = at;
    at += map->crackCount * 12;
    map->names = (const char *)at;
    at += namesSize;
    if (at > data + size || map->start >= map->locationCount)
        return -1;

    map->owned = calloc((map->itemCount + 7) / 8, 1);
    map->answers = calloc(map->varCount, 1);
    map->reached = calloc(map->locationCount, 1);
    map->queue = calloc(map->locationCount, 2);
    map->inLogic = calloc(map->checkCount, 1);
    map->inGlitched = calloc(map->checkCount, 1);
    if (!map->owned || !map->answers || !map->reached || !map->queue || !map->inLogic || !map->inGlitched)
        return -1;
    return 0;
}

MapCheck map_check(const MapData *map, u32 index)
{
    const u8 *at = map->checks + index * CHECK_SIZE;
    MapCheck check = {
        .rule = half(at),
        .item = half(at + 2),
        .quest = half(at + 4),
        .flag = half(at + 6),
        .flagCourse = at[8],
        .map = at[9],
        .x = at[10],
        .y = at[11],
        .course = at[12],
        .stage = at[13],
        .kind = at[14],
        .name = map->names + word(at + 16),
        .glitchRule = half(at + 20),
    };
    return check;
}

MapCrack map_crack(const MapData *map, u32 index)
{
    const u8 *at = map->cracks + index * 12;
    MapCrack crack = {
        .from = {at[0], at[1], at[2], at[3], at[4]},
        .to = {at[5], at[6], at[7], at[8], at[9]},
    };
    return crack;
}

static bool owns(const MapData *map, u16 item)
{
    return item < map->itemCount && (map->owned[item >> 3] & (1 << (item & 7)));
}

static void give(MapData *map, u16 item)
{
    if (item < map->itemCount)
        map->owned[item >> 3] |= 1 << (item & 7);
}

static u32 countGroup(const MapData *map, u16 group)
{
    if (group >= map->groupCount)
        return 0;
    const u8 *at = map->groups + group * 4;
    u16 start = half(at), length = half(at + 2);
    u32 count = 0;
    for (u16 i = 0; i < length; i++)
        count += owns(map, half(map->groupItems + (start + i) * 2));
    return count;
}

static void answerQuestions(MapData *map)
{
    u32 hearts = 12 + 4 * countGroup(map, map->specialGroups[0]) + countGroup(map, map->specialGroups[1]);
    u32 rupees = 50 * countGroup(map, map->specialGroups[2]) + 100 * countGroup(map, map->specialGroups[3]) +
                 300 * countGroup(map, map->specialGroups[4]);
    for (u32 i = 0; i < map->varCount; i++)
    {
        const u8 *at = map->vars + i * 6;
        u16 a = half(at + 2), b = half(at + 4);
        switch (at[0])
        {
        case 0: map->answers[i] = owns(map, a); break;
        case 1: map->answers[i] = countGroup(map, a) >= b; break;
        case 2: map->answers[i] = hearts >= b; break;
        default: map->answers[i] = rupees >= b; break;
        }
    }
}

static bool passes(const MapData *map, u16 node)
{
    for (u32 steps = 0; steps < 0x10000; steps++)
    {
        if (node == RULE_TRUE)
            return true;
        if (node == RULE_FALSE || node >= map->nodeCount)
            return false;
        const u8 *at = map->nodes + node * 6;
        u16 var = half(at);
        node = var < map->varCount && map->answers[var] ? half(at + 4) : half(at + 2);
    }
    return false;
}

// Finds the places and checks the player can reach with the items they own now, by the seed's
// rules or the glitched ones. Marks the checks in `found`.
static void reach(MapData *map, bool glitched, u8 *found)
{
    answerQuestions(map);
    memset(map->reached, 0, map->locationCount);
    memset(found, 0, map->checkCount);
    u32 head = 0, tail = 0;
    map->queue[tail++] = map->start;
    map->reached[map->start] = 1;
    while (head < tail)
    {
        u16 location = map->queue[head++];
        const u8 *at = map->locations + location * 8;
        u16 firstEdge = half(at), edges = half(at + 2), firstCheck = half(at + 4), checks = half(at + 6);
        for (u16 i = 0; i < checks; i++)
        {
            u32 check = firstCheck + i;
            if (check < map->checkCount)
                found[check] = passes(map, half(map->checks + check * CHECK_SIZE + (glitched ? 20 : 0)));
        }
        for (u16 i = 0; i < edges; i++)
        {
            const u8 *edge = map->edges + (firstEdge + i) * EDGE_SIZE;
            u16 to = half(edge);
            if (to < map->locationCount && !map->reached[to] && passes(map, half(edge + (glitched ? 4 : 2))))
            {
                map->reached[to] = 1;
                map->queue[tail++] = to;
            }
        }
    }
}

// Works out what the player owns from the checks they made, and what they can reach with it,
// until nothing new turns up.
static void settle(MapData *map, const u8 *checked, bool glitched, u8 *found)
{
    u32 bytes = (map->itemCount + 7) / 8;
    memset(map->owned, 0, bytes);
    u8 *before = malloc(bytes);
    for (int round = 0; round < 64; round++)
    {
        reach(map, glitched, found);
        if (before)
            memcpy(before, map->owned, bytes);
        memset(map->owned, 0, bytes);
        // The player has the items from the checks they made. Quest steps count once they can be
        // done, as do checks the game keeps no flag for.
        for (u32 i = 0; i < map->checkCount; i++)
        {
            MapCheck check = map_check(map, i);
            bool hasFlag = check.flagCourse != FLAG_NONE;
            if ((hasFlag && checked[i]) || (!hasFlag && found[i]))
                give(map, check.item);
            if (found[i])
                give(map, check.quest);
        }
        if (!before || memcmp(before, map->owned, bytes) == 0)
            break;
    }
    free(before);
}

void map_evaluate(MapData *map, const u8 *checked, u8 *status)
{
    settle(map, checked, false, map->inLogic);
    settle(map, checked, true, map->inGlitched);
    for (u32 i = 0; i < map->checkCount; i++)
    {
        MapCheck check = map_check(map, i);
        status[i] = check.flagCourse != FLAG_NONE && checked[i] ? STATUS_CHECKED
                    : map->inLogic[i]                         ? STATUS_IN_LOGIC
                    : map->inGlitched[i]                      ? STATUS_GLITCHED
                                                              : STATUS_OUT_OF_LOGIC;
    }
}

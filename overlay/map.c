// The logic map: an overlay on the bottom screen with a dot for each check of the seed, green when
// it is in logic, red when it is not, and grey once checked. Cracks the player has gone through
// show a line to where they lead. The game keeps running while the map is open.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "map_logic.h"
#include "overlay.h"

// The seed's files, in the game's Luma folder, where the active seed lives.
#define MAP_PATH "sdmc:/luma/titles/00040000000EC300/albw-map.bin"
#define CRACKS_PATH "sdmc:/luma/titles/00040000000EC300/albw-cracks.bin"

// Where the game keeps its state (USA version), as the Archipelago client reads it.
#define EVENTS_LOCATION 0x70b728
#define COURSES_LOCATION 0x70c8e0
#define SAVES_LOCATION 0x711de8
#define GAME_LOCATION 0x709df8

#define COURSE_COUNT 24
#define COURSE_FLAG_BYTES 0x30
#define EVENT_FLAG_BYTES 0x80

// The overlay, drawn once per change and copied over each frame: RGB565 in the screen's own layout.
static u16 image[320 * 240];
static const Canvas imageCanvas = {(u8 *)image, 480, 2, 2, 320};

static MapData map;
static bool loaded;
static u8 *checked, *status, *usedCracks;
static bool hasState;
static int shownWorld;
static u64 refreshedAt;
static u8 lastCourse = 0xFF;
static u32 lastStage;

static const char *const WORLD_NAMES[] = {"HYRULE", "LORULE"};

// The game's own map of each world, made by the app from the player's game files.
// The large ones are for the zoomed-in view.
static const struct
{
    const char *path;
    int size;
} PICTURES[2][2] = {
    {{"sdmc:/3ds/albw-rando/qol/map-hyrule.bin", 232}, {"sdmc:/3ds/albw-rando/qol/map-hyrule-large.bin", 464}},
    {{"sdmc:/3ds/albw-rando/qol/map-lorule.bin", 232}, {"sdmc:/3ds/albw-rando/qol/map-lorule-large.bin", 464}},
};
static u8 *pictures[2][2];

// Dungeons by the tag their checks' names start with, for the list on the top screen.
static const struct
{
    const char *tag, *name;
} DUNGEONS[] = {
    {"[EP]", "Eastern Palace"}, {"[HG]", "House of Gales"}, {"[TH]", "Tower of Hera"},  {"[HC]", "Hyrule Castle"},
    {"[PD]", "Dark Palace"},    {"[SP]", "Swamp Palace"},   {"[SW]", "Skull Woods"},    {"[TT]", "Thieves' Hideout"},
    {"[IR]", "Ice Ruins"},      {"[DP]", "Desert Palace"},  {"[TR]", "Turtle Rock"},    {"[LC]", "Lorule Castle"},
};
#define DUNGEON_COUNT (sizeof(DUNGEONS) / sizeof(DUNGEONS[0]))
static int dungeonCounts[DUNGEON_COUNT][4]; // by status: out of logic, in logic, checked, glitched

// Dungeons on each map, at their icon on the game's map, in map units.
static const struct
{
    u8 map, x, y;
    const char *label;
} LABELS[] = {
    {0, 250, 106, "EP"}, {0, 198, 223, "HG"}, {0, 156, 10, "TH"}, {0, 128, 116, "HC"}, {0, 18, 220, "DP"},
    {1, 252, 106, "PD"}, {1, 117, 238, "SP"}, {1, 42, 35, "SW"},  {1, 13, 132, "TT"}, {1, 234, 25, "IR"},
    {1, 204, 223, "TR"}, {1, 126, 120, "LC"},
};

static bool readable(u32 address, u32 size)
{
    MemInfo info;
    PageInfo page;
    return address >= 0x100000 && R_SUCCEEDED(svcQueryMemory(&info, &page, address)) && info.state != MEMSTATE_FREE &&
           (info.perm & MEMPERM_READ) && info.base_addr + info.size >= address + size;
}

static u32 readWord(u32 address)
{
    return readable(address, 4) ? *(vu32 *)address : 0;
}

static bool readFile(const char *path, u8 **data, u32 *size)
{
    FILE *file = fopen(path, "rb");
    if (!file)
        return false;
    fseek(file, 0, SEEK_END);
    *size = ftell(file);
    fseek(file, 0, SEEK_SET);
    *data = malloc(*size);
    bool ok = *data && fread(*data, 1, *size, file) == *size;
    fclose(file);
    return ok;
}

void map_load(void)
{
    u8 *data;
    u32 size;
    if (!readFile(MAP_PATH, &data, &size))
    {
        overlay_log("map: no %s", MAP_PATH);
        return;
    }
    if (map_parse(&map, data, size) != 0)
    {
        overlay_log("map: could not read the map file");
        return;
    }
    checked = calloc(map.checkCount, 1);
    status = calloc(map.checkCount, 1);
    usedCracks = calloc(map.crackCount + 1, 1);
    u8 *saved;
    u32 savedSize;
    if (readFile(CRACKS_PATH, &saved, &savedSize))
    {
        memcpy(usedCracks, saved, savedSize < map.crackCount ? savedSize : map.crackCount);
        free(saved);
    }
    loaded = checked && status && usedCracks;
    for (int world = 0; world < 2; world++)
        for (int large = 0; large < 2; large++)
        {
            u32 pictureSize;
            int size = PICTURES[world][large].size;
            if (readFile(PICTURES[world][large].path, &pictures[world][large], &pictureSize) && pictureSize != (u32)(size * size * 2))
            {
                free(pictures[world][large]);
                pictures[world][large] = NULL;
            }
        }
    overlay_log("map: %lu checks, %lu cracks", map.checkCount, map.crackCount);
}

// Reads which checks the game marks as done. False when no save is loaded.
static bool readChecks(void)
{
    static u8 events[EVENT_FLAG_BYTES], courses[COURSE_COUNT][COURSE_FLAG_BYTES];
    u32 eventsPtr = readWord(EVENTS_LOCATION), coursesPtr = readWord(COURSES_LOCATION), saves = readWord(SAVES_LOCATION);
    u32 save = saves ? readWord(saves + 0x14) : 0;
    if (!readable(eventsPtr + 0x48, EVENT_FLAG_BYTES) || !readable(coursesPtr + 0x160, COURSE_COUNT * 0x16c) ||
        !readable(save + 0x40, 0x600 + COURSE_COUNT * 0x40))
        return false;
    for (int i = 0; i < EVENT_FLAG_BYTES; i++)
        events[i] = *(vu8 *)(eventsPtr + 0x48 + i) | *(vu8 *)(save + 0x40 + i);
    for (int course = 0; course < COURSE_COUNT; course++)
    {
        u32 current = coursesPtr + course * 0x16c;
        for (int i = 0; i < COURSE_FLAG_BYTES; i++)
        {
            u8 now = i < 0x20 ? *(vu8 *)(current + 0x160 + i) : *(vu8 *)(current + 0x1a0 + i - 0x20);
            courses[course][i] = now | *(vu8 *)(save + 0x560 + course * 0x40 + i);
        }
    }
#define FLAG_SET(bytes, flag) ((flag) / 8 < sizeof(bytes) && ((bytes)[(flag) / 8] & (1 << ((flag) & 7))))
    for (u32 i = 0; i < map.checkCount; i++)
    {
        MapCheck check = map_check(&map, i);
        bool done = false;
        if (check.flagCourse == FLAG_EVENT)
            done = FLAG_SET(events, check.flag);
        else if (check.flagCourse < COURSE_COUNT)
        {
            done = FLAG_SET(courses[check.flagCourse], check.flag);
            // Like the Archipelago client: the overworld's high flags can also be kept by its
            // indoor and cave courses.
            if (check.flag >= 0x100 && check.flagCourse <= 1)
                done = done || FLAG_SET(courses[check.flagCourse + 2], check.flag) || FLAG_SET(courses[check.flagCourse + 4], check.flag);
        }
        checked[i] = done;
    }
    return true;
}

bool map_in_save(void)
{
    u32 saves = readWord(SAVES_LOCATION);
    u32 save = saves ? readWord(saves + 0x14) : 0;
    u32 player = readWord(0x70fb60);
    u32 game = readWord(GAME_LOCATION);
    // Like the Archipelago client: a save is in play when it has no error, and a player exists.
    // The title screen's demo is course 8.
    return save && readWord(save + 0x1600) == 0 && player && readWord(player + 0x10) && readable(game + 0x18, 4) &&
           *(vu8 *)(game + 0x18) != 8;
}

static int currentWorld(void)
{
    u32 game = readWord(GAME_LOCATION);
    if (!readable(game + 0x18, 8))
        return 0;
    switch (*(vu8 *)(game + 0x18))
    {
    case 1: case 3: case 5: case 0x0D: case 0x0E: case 0x0F: case 0x10: case 0x11: case 0x13: case 0x14: case 0x17:
        return 1;
    default:
        return 0;
    }
}

static void saveCracks(void)
{
    FILE *file = fopen(CRACKS_PATH, "wb");
    if (!file)
        return;
    fwrite(usedCracks, 1, map.crackCount, file);
    fclose(file);
}

static bool sameStage(u8 placeStage, u32 gameStage)
{
    // The region files number stages from 1; the game may count from 0.
    return placeStage == gameStage || placeStage == gameStage + 1;
}

// Notes cracks the player goes through, by watching the stage change from one crack's stage to
// where it leads.
static void stageChanged(u8 course, u32 stage);

static void watchCracks(void)
{
    u32 game = readWord(GAME_LOCATION);
    if (!readable(game + 0x18, 8))
        return;
    u32 saves = readWord(SAVES_LOCATION);
    u32 save = saves ? readWord(saves + 0x14) : 0;
    // On the title screen and in its demo, stage changes are not the player's, and the next
    // save loaded starts afresh.
    if (!save || readWord(save + 0x1600) != 0 || *(vu8 *)(game + 0x18) == 8)
    {
        lastCourse = 0xFF;
        return;
    }
    // While a stage loads there is no player; the change is seen once the new stage has one.
    u32 player = readWord(0x70fb60);
    if (!player || !readWord(player + 0x10))
        return;
    stageChanged(*(vu8 *)(game + 0x18), *(vu32 *)(game + 0x1c));
}

#ifdef PLUGIN_TEST
// Plays the stage changes of going through the first crack, as the game would report them, for
// the test to see the crack's line drawn.
void map_test_crack(void)
{
    if (!map.crackCount)
        return;
    MapCrack crack = map_crack(&map, 0);
    overlay_log("test crack: %u %u (%u,%u) -> %u %u (%u,%u)", crack.from.course, crack.from.stage, crack.from.x, crack.from.y,
                crack.to.course, crack.to.stage, crack.to.x, crack.to.y);
    stageChanged(crack.from.course, crack.from.stage - 1);
    stageChanged(crack.to.course, crack.to.stage - 1);
}
#endif

static void stageChanged(u8 course, u32 stage)
{
    if (course == lastCourse && stage == lastStage)
        return;
    if (lastCourse != 0xFF)
        overlay_log("stage: %u %lu -> %u %lu", lastCourse, lastStage, course, stage);
    bool changed = false;
    for (u32 i = 0; lastCourse != 0xFF && i < map.crackCount; i++)
    {
        MapCrack crack = map_crack(&map, i);
        if (usedCracks[i] || crack.from.course != lastCourse || !sameStage(crack.from.stage, lastStage) ||
            crack.to.course != course || !sameStage(crack.to.stage, stage))
            continue;
        usedCracks[i] = 1;
        changed = true;
        // The way back is known too.
        for (u32 j = 0; j < map.crackCount; j++)
        {
            MapCrack back = map_crack(&map, j);
            if (!memcmp(&back.from, &crack.to, sizeof(MapPlace)) && !memcmp(&back.to, &crack.from, sizeof(MapPlace)))
                usedCracks[j] = 1;
        }
    }
    if (changed)
    {
        saveCracks();
        refreshedAt = 0;
    }
    lastCourse = course;
    lastStage = stage;
}

// ---------------------------------------------------------------------------------------------
// Drawing.

#define MAP_LEFT 4
#define MAP_TOP 4
#define MAP_SIZE 232
#define PANEL_LEFT 242

// The view: 1 shows the whole world, 2 a quarter of it from viewX, viewY (map units).
static int zoom = 1, viewX, viewY;

static int screenX(int x) { return MAP_LEFT + (x - viewX) * MAP_SIZE * zoom / 256; }
static int screenY(int y) { return MAP_TOP + (y - viewY) * MAP_SIZE * zoom / 256; }

static void centerView(int x, int y)
{
    int span = 256 / zoom;
    viewX = x - span / 2;
    viewY = y - span / 2;
    viewX = viewX < 0 ? 0 : viewX > 256 - span ? 256 - span : viewX;
    viewY = viewY < 0 ? 0 : viewY > 256 - span ? 256 - span : viewY;
}

#define ZOOM_TOP 176
#define ZOOM_HEIGHT 22

static void line(int x0, int y0, int x1, int y1, u32 rgb)
{
    int dx = abs(x1 - x0), dy = -abs(y1 - y0), sx = x0 < x1 ? 1 : -1, sy = y0 < y1 ? 1 : -1, error = dx + dy;
    for (int steps = 0; steps < 1000; steps++)
    {
        canvas_fill(&imageCanvas, x0, y0, 1, 1, rgb);
        if (x0 == x1 && y0 == y1)
            break;
        int twice = 2 * error;
        if (twice >= dy) { error += dy; x0 += sx; }
        if (twice <= dx) { error += dx; y0 += sy; }
    }
}

#define GREEN 0x48E048
#define YELLOW 0xF0D040
#define RED 0xE04848
#define GREY 0x5A5A5A

// A marker's color: green if any of its checks are in logic, yellow if only glitches reach one,
// red if checks are left that need more items, grey when all are done.
static u32 statusColor(int in, int glitched, int out)
{
    return in ? GREEN : glitched ? YELLOW : out ? RED : GREY;
}

extern u32 overlayVersion;

// The place the player tapped, whose checks are listed until they tap somewhere else.
static bool selected;
static u8 selectedX, selectedY;

static bool atSelection(const MapCheck *check)
{
    return check->kind == KIND_ITEM && check->map == shownWorld && check->x == selectedX && check->y == selectedY;
}

// Lists the tapped place's checks in a box over the map, each in its status color.
static void drawSelection(const Canvas *c)
{
    if (!selected)
        return;
    int count = 0;
    for (u32 i = 0; i < map.checkCount; i++)
    {
        MapCheck check = map_check(&map, i);
        count += atSelection(&check);
    }
    if (!count)
        return;
    const int lines = count > 18 ? 18 : count, height = lines * 10 + 6;
    // The box goes on the half of the map away from the place, so the dot stays in view.
    int top = screenY(selectedY) < MAP_TOP + MAP_SIZE / 2 ? MAP_TOP + MAP_SIZE - height : MAP_TOP;
    canvas_fill(c, MAP_LEFT, top, MAP_SIZE, height, 0x000000);
    canvas_fill(c, MAP_LEFT + 1, top + 1, MAP_SIZE - 2, height - 2, 0x101828);
    int x = screenX(selectedX), y = screenY(selectedY);
    canvas_fill(c, x - 4, y - 4, 9, 1, 0xFFFFFF);
    canvas_fill(c, x - 4, y + 4, 9, 1, 0xFFFFFF);
    canvas_fill(c, x - 4, y - 4, 1, 9, 0xFFFFFF);
    canvas_fill(c, x + 4, y - 4, 1, 9, 0xFFFFFF);
    int line = 0;
    for (u32 i = 0; i < map.checkCount && line < lines; i++)
    {
        MapCheck check = map_check(&map, i);
        if (!atSelection(&check))
            continue;
        static const u32 colors[] = {0xF06060, 0x60F060, 0x909090, 0xF0E060};
        u32 color = colors[status[i] & 3];
        char name[30];
        // Dungeon checks drop their "[EP] " tag; the box is for one place anyway.
        const char *text = check.name[0] == '[' && check.name[3] == ']' ? check.name + 5 : check.name;
        if (line == lines - 1 && count > lines)
            snprintf(name, sizeof(name), "and %d more", count - lines + 1);
        else
            snprintf(name, sizeof(name), "%.28s", text);
        canvas_text(c, MAP_LEFT + 4, top + 4 + line * 10, name, color);
        line++;
    }
}

static void compose(void)
{
    overlayVersion++;
    const Canvas *c = &imageCanvas;
    canvas_fill(c, 0, 0, 320, 240, 0x0C1420);
    canvas_fill(c, MAP_LEFT - 1, MAP_TOP - 1, MAP_SIZE + 2, MAP_SIZE + 2, 0x24344A);
    int large = zoom > 1 && pictures[shownWorld][1];
    if (pictures[shownWorld][large])
    {
        const u8 *picture = pictures[shownWorld][large];
        int size = PICTURES[shownWorld][large].size, scale = 256 * MAP_SIZE * zoom;
        for (int y = 0; y < MAP_SIZE; y++)
        {
            int py = (viewY * size * MAP_SIZE * zoom + y * size * 256) / scale;
            for (int x = 0; x < MAP_SIZE; x++)
            {
                int px = (viewX * size * MAP_SIZE * zoom + x * size * 256) / scale;
                const u8 *pixel = picture + (py * size + px) * 2;
                image[(MAP_LEFT + x) * 240 + 239 - (MAP_TOP + y)] = pixel[0] | pixel[1] << 8;
            }
        }
    }
    else
    {
        canvas_fill(c, MAP_LEFT, MAP_TOP, MAP_SIZE, MAP_SIZE, shownWorld ? 0x1A1424 : 0x14201A);
        // The game's map is a grid of 32-unit areas.
        for (int i = 1; i < 8; i++)
        {
            canvas_fill(c, screenX(i * 32), MAP_TOP, 1, MAP_SIZE, shownWorld ? 0x261E34 : 0x1E2E26);
            canvas_fill(c, MAP_LEFT, screenY(i * 32), MAP_SIZE, 1, shownWorld ? 0x261E34 : 0x1E2E26);
        }
    }
    for (unsigned i = 0; i < sizeof(LABELS) / sizeof(LABELS[0]); i++)
        if (LABELS[i].map == shownWorld)
            canvas_text(c, screenX(LABELS[i].x) - 8, screenY(LABELS[i].y) + 5, LABELS[i].label, 0x8090A8);

    int totalIn = 0, totalOut = 0, totalDone = 0, totalGlitched = 0;
    memset(dungeonCounts, 0, sizeof(dungeonCounts));
    if (loaded && hasState)
    {
        for (u32 i = 0; i < map.checkCount; i++)
        {
            MapCheck check = map_check(&map, i);
            if (check.kind != KIND_ITEM)
                continue;
            for (unsigned d = 0; d < DUNGEON_COUNT; d++)
                if (!strncmp(check.name, DUNGEONS[d].tag, 4))
                    dungeonCounts[d][status[i]]++;
        }
        for (u32 i = 0; i < map.checkCount; i++)
        {
            MapCheck check = map_check(&map, i);
            if (check.kind != KIND_ITEM)
                continue;
            totalIn += status[i] == STATUS_IN_LOGIC;
            totalOut += status[i] == STATUS_OUT_OF_LOGIC;
            totalGlitched += status[i] == STATUS_GLITCHED;
            totalDone += status[i] == STATUS_CHECKED;
            if (check.map != shownWorld)
                continue;
            // Checks in the same place, such as a dungeon's, share one marker.
            bool first = true;
            for (u32 j = 0; j < i && first; j++)
            {
                MapCheck other = map_check(&map, j);
                first = !(other.kind == KIND_ITEM && other.map == check.map && other.x == check.x && other.y == check.y);
            }
            if (!first)
                continue;
            int in = 0, out = 0, done = 0, glitched = 0;
            for (u32 j = i; j < map.checkCount; j++)
            {
                MapCheck other = map_check(&map, j);
                if (other.kind == KIND_ITEM && other.map == check.map && other.x == check.x && other.y == check.y)
                {
                    in += status[j] == STATUS_IN_LOGIC;
                    out += status[j] == STATUS_OUT_OF_LOGIC;
                    done += status[j] == STATUS_CHECKED;
                    glitched += status[j] == STATUS_GLITCHED;
                }
            }
            int x = screenX(check.x), y = screenY(check.y);
            u32 color = statusColor(in, glitched, out);
            if (in + out + done + glitched > 1)
            {
                canvas_fill(c, x - 3, y - 3, 7, 7, 0x000000);
                canvas_fill(c, x - 2, y - 2, 5, 5, color);
                if (in)
                {
                    char count[4];
                    snprintf(count, sizeof(count), "%d", in > 99 ? 99 : in);
                    canvas_text(c, x + 4, y - 4, count, 0xFFFFFF);
                }
            }
            else
            {
                canvas_fill(c, x - 2, y - 2, 4, 4, 0x000000);
                canvas_fill(c, x - 1, y - 1, 3, 3, color);
            }
        }
        // Cracks: a purple mark, and once used, a line to where it leads.
        for (u32 i = 0; i < map.crackCount; i++)
        {
            MapCrack crack = map_crack(&map, i);
            if (crack.from.map != shownWorld)
                continue;
            int x = screenX(crack.from.x), y = screenY(crack.from.y);
            if (usedCracks[i])
            {
                u32 color = crack.to.map == crack.from.map ? 0xC080FF : 0x40D0FF;
                line(x, y, screenX(crack.to.x), screenY(crack.to.y), color);
                canvas_fill(c, screenX(crack.to.x) - 1, screenY(crack.to.y) - 1, 3, 3, color);
            }
            canvas_fill(c, x - 1, y - 3, 3, 7, 0xA060E0);
            canvas_fill(c, x - 3, y - 1, 7, 3, 0xA060E0);
        }
        drawSelection(c);
    }
    // Markers near the edge spill over; the frame and panel are drawn over them.
    canvas_fill(c, 0, 0, 320, MAP_TOP - 1, 0x0C1420);
    canvas_fill(c, 0, MAP_TOP + MAP_SIZE + 1, 320, 240, 0x0C1420);
    canvas_fill(c, 0, 0, MAP_LEFT - 1, 240, 0x0C1420);
    canvas_fill(c, MAP_LEFT + MAP_SIZE + 1, 0, 320, 240, 0x0C1420);

    // The zoom button.
    canvas_fill(c, PANEL_LEFT - 2, ZOOM_TOP, 320 - PANEL_LEFT, ZOOM_HEIGHT, 0x24344A);
    canvas_text(c, PANEL_LEFT + 2, ZOOM_TOP + 7, zoom > 1 ? "Zoom out" : "Zoom in", 0xFFFFFF);

    int y = 6;
    canvas_text(c, PANEL_LEFT, y, WORLD_NAMES[shownWorld], 0xFFFFFF);
    y += 16;
    if (!loaded)
    {
        canvas_text(c, PANEL_LEFT, y, "No map", 0xE0C060);
        canvas_text(c, PANEL_LEFT, y + 10, "for this", 0xE0C060);
        canvas_text(c, PANEL_LEFT, y + 20, "seed.", 0xE0C060);
        return;
    }
    if (!hasState)
    {
        canvas_text(c, PANEL_LEFT, y, "Load a", 0xE0C060);
        canvas_text(c, PANEL_LEFT, y + 10, "save file", 0xE0C060);
        return;
    }
    char text[16];
    canvas_fill(c, PANEL_LEFT, y + 1, 6, 6, 0x48E048);
    snprintf(text, sizeof(text), "%d", totalIn);
    canvas_text(c, PANEL_LEFT + 10, y, text, 0xFFFFFF);
    canvas_text(c, PANEL_LEFT + 10, y + 10, "in logic", 0xA0A8B8);
    y += 26;
    if (totalGlitched)
    {
        canvas_fill(c, PANEL_LEFT, y + 1, 6, 6, YELLOW);
        snprintf(text, sizeof(text), "%d", totalGlitched);
        canvas_text(c, PANEL_LEFT + 10, y, text, 0xFFFFFF);
        canvas_text(c, PANEL_LEFT + 10, y + 10, "glitches", 0xA0A8B8);
        y += 26;
    }
    canvas_fill(c, PANEL_LEFT, y + 1, 6, 6, 0xE04848);
    snprintf(text, sizeof(text), "%d", totalOut);
    canvas_text(c, PANEL_LEFT + 10, y, text, 0xFFFFFF);
    canvas_text(c, PANEL_LEFT + 10, y + 10, "not yet", 0xA0A8B8);
    y += 26;
    canvas_fill(c, PANEL_LEFT, y + 1, 6, 6, 0x5A5A5A);
    snprintf(text, sizeof(text), "%d", totalDone);
    canvas_text(c, PANEL_LEFT + 10, y, text, 0xFFFFFF);
    canvas_text(c, PANEL_LEFT + 10, y + 10, "checked", 0xA0A8B8);
    y += 26;
    if (map.crackCount)
    {
        canvas_fill(c, PANEL_LEFT + 2, y - 1, 3, 9, 0xA060E0);
        canvas_fill(c, PANEL_LEFT, y + 2, 7, 3, 0xA060E0);
        canvas_text(c, PANEL_LEFT + 10, y, "crack", 0xA0A8B8);
        y += 12;
        canvas_fill(c, PANEL_LEFT, y + 3, 7, 1, 0x40D0FF);
        canvas_text(c, PANEL_LEFT + 10, y, "to other", 0xA0A8B8);
        canvas_text(c, PANEL_LEFT + 10, y + 10, "world", 0xA0A8B8);
        y += 24;
    }
    canvas_text(c, PANEL_LEFT, 208, "Hold:", 0x708098);
    canvas_text(c, PANEL_LEFT, 218, shownWorld ? "Hyrule" : "Lorule", 0x708098);
    canvas_text(c, PANEL_LEFT, 230, "Press:shut", 0x708098);
}

static void buildPanel(void);

void map_refresh(void)
{
    hasState = loaded && readChecks();
#ifdef PLUGIN_TEST
    // Without a save loaded, tests show the seed as it starts.
    if (loaded && !hasState)
        hasState = true;
#endif
    if (hasState)
        map_evaluate(&map, checked, status);
    compose();
    buildPanel();
}

void map_open(void)
{
    selected = false;
    zoom = 1;
    centerView(128, 128);
    shownWorld = currentWorld();
    map_refresh();
    refreshedAt = svcGetSystemTick();
}

void map_touch(int x, int y)
{
    if (x >= PANEL_LEFT - 2 && y >= ZOOM_TOP && y < ZOOM_TOP + ZOOM_HEIGHT)
    {
        // Zooming in centers on the listed place, or else the middle of the view.
        int cx = selected ? selectedX : viewX + 128 / zoom, cy = selected ? selectedY : viewY + 128 / zoom;
        zoom = zoom > 1 ? 1 : 2;
        centerView(cx, cy);
        compose();
        return;
    }
    if (!loaded || !hasState)
        return;
    // The nearest marker within reach of the tap, or none to hide the list.
    int best = 8 * 8 + 1;
    bool found = false;
    for (u32 i = 0; i < map.checkCount; i++)
    {
        MapCheck check = map_check(&map, i);
        if (check.kind != KIND_ITEM || check.map != shownWorld)
            continue;
        int dx = screenX(check.x) - x, dy = screenY(check.y) - y;
        if (dx * dx + dy * dy < best)
        {
            best = dx * dx + dy * dy;
            selectedX = check.x;
            selectedY = check.y;
            found = true;
        }
    }
    selected = found;
    // Zoomed in, a tap away from the markers moves the view there.
    if (!found && zoom > 1 && x >= MAP_LEFT && x < MAP_LEFT + MAP_SIZE && y >= MAP_TOP && y < MAP_TOP + MAP_SIZE)
        centerView(viewX + (x - MAP_LEFT) * 256 / (MAP_SIZE * zoom), viewY + (y - MAP_TOP) * 256 / (MAP_SIZE * zoom));
    compose();
}

void map_switch_world(void)
{
    selected = false;
    shownWorld = !shownWorld;
    compose();
}

// The dungeon list, drawn on the top screen while the map is open.
#define PANEL_WIDTH 206
#define PANEL_TOP 18
#define PANEL_HEIGHT (16 + DUNGEON_COUNT * 11)

static void drawDungeons(const Canvas *canvas)
{
    const int width = PANEL_WIDTH, left = canvas->width - width, top = PANEL_TOP;
    canvas_fill(canvas, left, top, width, PANEL_HEIGHT, 0x0C1420);
    canvas_text(canvas, left + 4, top + 4, "DUNGEONS", 0xFFFFFF);
    canvas_text(canvas, left + 150, top + 4, "in", 0x48E048);
    canvas_text(canvas, left + 174, top + 4, "out", 0xE04848);
    for (unsigned d = 0; d < DUNGEON_COUNT; d++)
    {
        int y = top + 16 + d * 11;
        int in = dungeonCounts[d][STATUS_IN_LOGIC], glitched = dungeonCounts[d][STATUS_GLITCHED];
        int out = dungeonCounts[d][STATUS_OUT_OF_LOGIC] + glitched;
        u32 color = statusColor(in, glitched, out);
        canvas_fill(canvas, left + 4, y + 1, 6, 6, color);
        char name[18];
        snprintf(name, sizeof(name), "%.16s", DUNGEONS[d].name);
        canvas_text(canvas, left + 14, y, name, in + out ? 0xFFFFFF : 0x7A7A7A);
        if (!hasState)
            continue;
        char count[8];
        snprintf(count, sizeof(count), "%2d", in);
        canvas_text(canvas, left + 148, y, count, 0x48E048);
        snprintf(count, sizeof(count), "%2d", out);
        canvas_text(canvas, left + 178, y, count, 0xE04848);
    }
}

// The dungeon list is drawn once into this picture, laid out like an RGB565 framebuffer, and
// copied onto each new frame of the game; drawing its text every frame slowed the game.
static u16 panel[PANEL_WIDTH * 240];

static void buildPanel(void)
{
    const Canvas canvas = {(u8 *)panel, 480, 2, 2, PANEL_WIDTH};
    drawDungeons(&canvas);
}

void map_draw_dungeons(const Canvas *canvas)
{
    if (canvas->format != 2 || canvas->bpp != 2 || canvas->stride != 480 || canvas->width < PANEL_WIDTH)
    {
        drawDungeons(canvas);
        return;
    }
    // Rows top to bottom of the screen run backwards in memory.
    u32 first = 239 - (PANEL_TOP + PANEL_HEIGHT - 1);
    for (int x = 0; x < PANEL_WIDTH; x++)
        memcpy(canvas->pixels + (canvas->width - PANEL_WIDTH + x) * 480 + first * 2, panel + x * 240 + first,
               PANEL_HEIGHT * 2);
}

void map_tick(u64 now, bool open)
{
    if (!loaded)
        return;
    watchCracks();
    // While open, the map follows the game about once a second.
    if (open && now - refreshedAt > 268111ULL * 1000)
    {
        refreshedAt = now;
        map_refresh();
    }
}

void map_draw(const Canvas *canvas)
{
    if (canvas->format == 2 && canvas->stride == 480 && canvas->width == 320)
    {
        memcpy(canvas->pixels, image, sizeof(image));
        return;
    }
    for (int x = 0; x < 320 && x < canvas->width; x++)
        for (int y = 0; y < 240; y++)
        {
            u16 p = image[x * 240 + y];
            u32 rgb = ((p >> 11) << 19) | (((p >> 5) & 0x3F) << 10) | ((p & 0x1F) << 3);
            canvas_fill(canvas, x, 239 - y, 1, 1, rgb);
        }
}

#ifdef PLUGIN_TEST
void map_dump(const char *path)
{
    // The top screen's dungeon list, drawn on black.
    static u16 top[400 * 240];
    const Canvas topCanvas = {(u8 *)top, 480, 2, 2, 400};
    canvas_fill(&topCanvas, 0, 0, 400, 240, 0);
    map_draw_dungeons(&topCanvas);
    FILE *topFile = fopen("sdmc:/3ds/albw-rando/test-dungeons.raw", "wb");
    if (topFile)
    {
        fwrite(top, 1, sizeof(top), topFile);
        fclose(topFile);
    }
    FILE *file = fopen(path, "wb");
    if (file)
    {
        fwrite(image, 1, sizeof(image), file);
        fclose(file);
    }
}
#endif

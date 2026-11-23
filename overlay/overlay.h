// Shared parts of the QoL overlay plugin.
#pragma once

#include <3ds.h>

// What ZL or ZR does. The app's QoL page sets these.
enum
{
    ACTION_NONE = 0,
    ACTION_MAP = 1,   // open or close the logic map
    ACTION_TIMER = 2, // pause or start the timer; hold to reset it
};

typedef struct
{
    u32 timer; // show the speedrun timer
    u32 zl;
    u32 zr;
} Settings;

extern Settings settings;

// A screen's framebuffer as the game left it, ready to draw on. Coordinates are as the player
// sees the screen: x across (400 or 320), y down (240).
typedef struct
{
    u8 *pixels;
    u32 stride;
    u32 bpp;
    u32 format;
    int width;
} Canvas;

void canvas_fill(const Canvas *canvas, int x, int y, int w, int h, u32 rgb);
int canvas_text(const Canvas *canvas, int x, int y, const char *text, u32 rgb);
void overlay_log(const char *format, ...);

// The logic map (map.c).
void map_load(void);
void map_refresh(void);
void map_open(void);
void map_switch_world(void);
// The player touched the bottom screen at x, y while the map is open.
void map_touch(int x, int y);
void map_draw_dungeons(const Canvas *canvas);
// True while a save file is being played, not on the title screen.
bool map_in_save(void);
// Called often: follows cracks the player uses, and keeps an open map up to date.
void map_tick(u64 now, bool open);
void map_draw(const Canvas *canvas);

// Memory access over the network for the Archipelago client (rpc.c).
bool rpc_start(void);
// Called often: answers the client's requests.
void rpc_poll(void);
// The client command to show until the client connects, or NULL.
const char *rpc_banner(void);

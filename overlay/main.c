// QoL overlay plugin for the ALBW Randomizer app.
//
// The app asks Luma's plugin loader to load this plugin into A Link Between Worlds when the player
// starts the game from the app. The plugin runs in its own thread beside the game. It can show a
// speedrun timer that leaves out loading screens, and a map of the seed's checks that shows which
// ones are in logic. ZL and ZR on the New 3DS do what the player chose in the app.
//
// The game never waits for this plugin. The plugin reads the screens' framebuffer addresses from
// the GPU, and after the game shows a new frame it draws over that frame.

#include <3ds.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "overlay.h"

// The app's request, in the plugin header's config words (see the app's job.rs).
#define REQUEST_MAGIC 0x57424C41 // "ALBW"
#define REQUEST_VERSION 2

typedef struct
{
    u32 magic;
    u32 version;
    u32 heapVA;
    u32 heapSize;
    u32 exeSize;
    u32 isDefaultPlugin;
    s32 *event;
    s32 *reply;
    u32 reserved[24];
    u32 config[32];
} PluginHeader;

#define HEADER ((PluginHeader *)0x07000000)

Settings settings;

// What each try to start the thread gave, for the log.
static Result threadResult[4];

// ---------------------------------------------------------------------------------------------
// Memory: newlib's heap is the memory Luma gives the plugin, never the game's.

extern char *fake_heap_start;
extern char *fake_heap_end;
extern u32 __ctru_heap, __ctru_heap_size, __ctru_linear_heap, __ctru_linear_heap_size;

void __system_allocateHeaps(void)
{
    __ctru_heap = HEADER->heapVA;
    __ctru_heap_size = HEADER->heapSize;
    __ctru_linear_heap = 0;
    __ctru_linear_heap_size = 0;
    fake_heap_start = (char *)HEADER->heapVA;
    fake_heap_end = fake_heap_start + HEADER->heapSize;
}

void __system_initArgv(void) {}

// ---------------------------------------------------------------------------------------------
// Settings.

#define SAVED_SETTINGS "sdmc:/3ds/albw-rando/qol.txt"
#define MAP_FILE "sdmc:/luma/titles/00040000000EC300/albw-map.bin"
// Where an Archipelago patch puts its header, starting "ARCH" (see the apworld's Client.py).
#define AP_HEADER_LOCATION 0x6fe5f8

static bool launchedByApp;
static bool timerRunning = true;

static void settingsChosen(void)
{
    if (settings.zl > ACTION_TIMER)
        settings.zl = ACTION_NONE;
    if (settings.zr > ACTION_TIMER)
        settings.zr = ACTION_NONE;
    // With a button for the timer, the player starts it. Without one, it runs from the start.
    timerRunning = settings.zl != ACTION_TIMER && settings.zr != ACTION_TIMER;
}

// The app saves the choices as three numbers: timer, ZL action, ZR action.
static void readSavedSettings(void)
{
    FILE *file = fopen(SAVED_SETTINGS, "r");
    unsigned long timer = 0, zl = 0, zr = 0;
    if (file)
    {
        if (fscanf(file, "%lu %lu %lu", &timer, &zl, &zr) != 3)
            timer = zl = zr = 0;
        fclose(file);
    }
    settings.timer = timer != 0;
    settings.zl = zl;
    settings.zr = zr;
    settingsChosen();
}

// ---------------------------------------------------------------------------------------------
// Log. Every build writes a short one, so a player can send it when the plugin misbehaves on a
// console. Test builds also log every button change and loading screen.

#define LOG_FILE "sdmc:/3ds/albw-rando/overlay.log"

static FILE *logFile;

void overlay_log(const char *format, ...)
{
    if (!logFile)
        return;
    va_list args;
    va_start(args, format);
    vfprintf(logFile, format, args);
    va_end(args);
    fputc('\n', logFile);
    fflush(logFile);
}

// ---------------------------------------------------------------------------------------------
// Services, called directly, since libctru's own setup would map memory the game may need.

static Handle gspHandle, hidHandle, irrstHandle;
static vu32 *hidMemory, *irrstMemory;

// With `viaCommandBuffer`, the reply lands in the second half of this thread's command buffer
// and is copied out. A real 3DS refused the reply into the plugin's own memory, which Luma maps
// in a way IPC may not accept. Azahar overwrites the command buffer, so the plugin's memory is
// tried first.
static Result readRegisters(u32 address, void *out, u32 size, bool viaCommandBuffer)
{
    u32 *cmdbuf = getThreadCommandBuffer();
    u32 *staticbufs = getThreadStaticBuffers();
    u32 *landing = viaCommandBuffer ? cmdbuf + 32 : out;
    if (size > 0x80)
        return -1;
    u32 saved[2] = {staticbufs[0], staticbufs[1]};
    staticbufs[0] = IPC_Desc_StaticBuffer(size, 0);
    staticbufs[1] = (u32)landing;
    cmdbuf[0] = IPC_MakeHeader(0x4, 2, 0);
    cmdbuf[1] = address;
    cmdbuf[2] = size;
    Result res = svcSendSyncRequest(gspHandle);
    if (R_SUCCEEDED(res))
        res = cmdbuf[1];
    if (R_SUCCEEDED(res) && viaCommandBuffer)
        memcpy(out, landing, size);
    staticbufs[0] = saved[0];
    staticbufs[1] = saved[1];
    return res;
}

// Maps a shared memory block at a free address in the area apps map shared memory into. The
// game picks its own addresses, so this one searches down from the top of the area.
static vu32 *mapShared(Handle block, u32 size)
{
    for (u32 address = 0x13F00000; address > 0x12000000; address -= 0x10000)
    {
        MemInfo info;
        PageInfo page;
        if (R_FAILED(svcQueryMemory(&info, &page, address)) || info.state != MEMSTATE_FREE)
            continue;
        if (info.base_addr + info.size < address + size)
            continue;
        if (R_SUCCEEDED(svcMapMemoryBlock(block, address, MEMPERM_READ, MEMPERM_DONTCARE)))
            return (vu32 *)address;
    }
    return NULL;
}

static void startInput(void)
{
    if (R_SUCCEEDED(srvGetServiceHandle(&hidHandle, "hid:USER")))
    {
        u32 *cmdbuf = getThreadCommandBuffer();
        cmdbuf[0] = IPC_MakeHeader(0xA, 0, 0);
        if (R_SUCCEEDED(svcSendSyncRequest(hidHandle)) && R_SUCCEEDED(cmdbuf[1]))
        {
            Handle memory = cmdbuf[3];
            for (int i = 4; i < 9; i++)
                svcCloseHandle(cmdbuf[i]);
            hidMemory = mapShared(memory, 0x2B0);
        }
    }
    // ZL and ZR come from ir:rst, which only the New 3DS has (or the Circle Pad Pro).
    srvSetBlockingPolicy(true);
    Result res = srvGetServiceHandle(&irrstHandle, "ir:rst");
    srvSetBlockingPolicy(false);
    if (R_SUCCEEDED(res))
    {
        u32 *cmdbuf = getThreadCommandBuffer();
        cmdbuf[0] = IPC_MakeHeader(0x1, 0, 0);
        if (R_SUCCEEDED(svcSendSyncRequest(irrstHandle)) && R_SUCCEEDED(cmdbuf[1]))
        {
            Handle memory = cmdbuf[3];
            svcCloseHandle(cmdbuf[4]);
            irrstMemory = mapShared(memory, 0x98);
            cmdbuf = getThreadCommandBuffer();
            cmdbuf[0] = IPC_MakeHeader(0x2, 2, 0);
            cmdbuf[1] = 10; // update period, ms
            cmdbuf[2] = 0;
            svcSendSyncRequest(irrstHandle);
        }
    }
    overlay_log("input: hid %p, ir:rst %p", hidMemory, irrstMemory);
}

// Where the bottom screen is touched, or false when it is not.
static bool readTouch(int *x, int *y)
{
    if (!hidMemory)
        return false;
    u32 id = hidMemory[46] & 7;
    u32 position = hidMemory[50 + id * 2], valid = hidMemory[51 + id * 2];
    *x = position & 0xFFFF;
    *y = position >> 16;
    return valid != 0;
}

static u32 readKeys(void)
{
    u32 keys = 0;
    if (hidMemory)
    {
        u32 id = hidMemory[4] & 7;
        keys |= hidMemory[10 + id * 4];
    }
    if (irrstMemory)
    {
        u32 id = irrstMemory[4] & 7;
        keys |= irrstMemory[6 + id * 4] & (KEY_ZL | KEY_ZR);
    }
    return keys;
}

// ---------------------------------------------------------------------------------------------
// Screens.

typedef struct
{
    u32 address[2]; // the two buffers, physical
    u32 right[2];   // the top screen's right-eye buffers
    u32 format;
    u32 select;
    u32 stride;
    u32 shown;      // physical address of the buffer on screen now
} Framebuffer;

// The physical address behind one of the game's framebuffer addresses.
static u32 physicalOf(u32 virtual)
{
    if (virtual >= 0x1F000000 && virtual < 0x1F600000)
        return virtual - 0x1F000000 + 0x18000000;
    if (virtual >= 0x14000000 && virtual < 0x1C000000)
        return virtual - 0x14000000 + 0x20000000;
    if (virtual >= 0x30000000 && virtual < 0x38000000)
        return virtual - 0x30000000 + 0x20000000;
    return 0;
}

// The GSP shared memory the game set up, and the game's slot in it. Each slot holds the
// framebuffers the game last asked to show, in the layout 3dbrew documents.
static vu8 *gspShared;
static int gspSlot = -1;

static vu32 *sharedInfo(vu8 *shared, int slot, bool top)
{
    vu8 *info = shared + 0x200 + slot * 0x80 + (top ? 0 : 0x40);
    return (vu32 *)(info + 4 + (info[0] & 1) * 0x1C);
}

static bool findGspShared(void)
{
    static bool searched;
    if (searched)
        return gspShared != NULL;
    searched = true;
    for (u32 address = 0x10000000; address < 0x14000000;)
    {
        MemInfo info;
        PageInfo page;
        if (R_FAILED(svcQueryMemory(&info, &page, address)))
            break;
        if (info.state == MEMSTATE_SHARED && (info.perm & MEMPERM_WRITE) && info.size >= 0x1000)
        {
            vu8 *shared = (vu8 *)info.base_addr;
            for (int slot = 0; slot < 4; slot++)
            {
                if (physicalOf(sharedInfo(shared, slot, true)[1]) && physicalOf(sharedInfo(shared, slot, false)[1]))
                {
                    gspShared = shared;
                    gspSlot = slot;
                    overlay_log("gsp shared memory at %08lx, slot %d", info.base_addr, slot);
                    return true;
                }
            }
        }
        address = info.base_addr + info.size;
    }
    overlay_log("gsp shared memory: none found");
    return false;
}

// Reads the framebuffers from the game's GSP shared memory, for when the GPU registers cannot be
// read.
static bool readSharedFramebuffer(bool top, Framebuffer *fb)
{
    if (!findGspShared())
        return false;
    vu32 *info = sharedInfo(gspShared, gspSlot, top);
    u32 left = physicalOf(info[1]);
    if (!left)
        return false;
    u32 select = info[0] & 1;
    fb->select = select;
    fb->address[select] = left;
    fb->right[select] = top ? physicalOf(info[2]) : 0;
    fb->stride = info[3];
    fb->format = info[4];
    fb->shown = left;
    return true;
}

static bool readFramebuffer(bool top, Framebuffer *fb)
{
    // 0: reply into the plugin's memory, 1: through the command buffer, 2: GSP shared memory.
#ifdef PLUGIN_FORCE_SHARED
    static int way = 2;
#else
    static int way;
#endif
    u32 regs[14];
    while (way < 2)
    {
        Result res = readRegisters(top ? 0x400468 : 0x400568, regs, top ? 0x38 : 0x2C, way == 1);
        if (R_SUCCEEDED(res))
            break;
        overlay_log("GPU registers refused (way %d): %08lx", way, res);
        way++;
    }
    if (way == 2)
        return readSharedFramebuffer(top, fb);
    fb->address[0] = regs[0];
    fb->address[1] = regs[1];
    fb->format = regs[2];
    fb->select = regs[4];
    fb->stride = regs[10];
    fb->right[0] = top ? regs[11] : 0;
    fb->right[1] = top ? regs[12] : 0;
    fb->shown = fb->address[fb->select & 1];
    return true;
}

// The address of a framebuffer in this process, or NULL when the game has no mapping for it
// (as when the HOME Menu's own buffers are on screen).
static u8 *virtualAddress(u32 physical, u32 size)
{
    u32 candidates[2];
    int count = 0;
    if (physical >= 0x18000000 && physical < 0x18600000)
        candidates[count++] = physical - 0x18000000 + 0x1F000000;
    else if (physical >= 0x20000000 && physical < 0x28000000)
    {
        candidates[count++] = physical - 0x20000000 + 0x30000000;
        candidates[count++] = physical - 0x20000000 + 0x14000000;
    }
    for (int i = 0; i < count; i++)
    {
        MemInfo info;
        PageInfo page;
        if (R_SUCCEEDED(svcQueryMemory(&info, &page, candidates[i])) && info.state != MEMSTATE_FREE &&
            (info.perm & MEMPERM_WRITE) && info.base_addr + info.size >= candidates[i] + size)
            return (u8 *)candidates[i];
    }
    return NULL;
}

static bool canvasFor(const Framebuffer *fb, u32 physical, int width, Canvas *canvas)
{
    static const u8 bytes[] = {4, 3, 2, 2, 2};
    u32 format = fb->format & 7;
    if (format > 4 || physical == 0)
        return false;
    canvas->bpp = bytes[format];
    canvas->format = format;
    canvas->stride = fb->stride ? fb->stride : 240 * canvas->bpp;
    canvas->width = width;
    canvas->pixels = virtualAddress(physical, canvas->stride * width);
    return canvas->pixels != NULL;
}

static u32 packColor(const Canvas *canvas, u32 rgb)
{
    u32 r = (rgb >> 16) & 0xFF, g = (rgb >> 8) & 0xFF, b = rgb & 0xFF;
    switch (canvas->format)
    {
    case 0: return (r << 24) | (g << 16) | (b << 8) | 0xFF;
    case 1: return (r << 16) | (g << 8) | b;
    case 2: return ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
    case 3: return ((r >> 3) << 11) | ((g >> 3) << 6) | ((b >> 3) << 1) | 1;
    default: return ((r >> 4) << 12) | ((g >> 4) << 8) | ((b >> 4) << 4) | 0xF;
    }
}

void canvas_fill(const Canvas *canvas, int x, int y, int w, int h, u32 rgb)
{
    if (x < 0) { w += x; x = 0; }
    if (y < 0) { h += y; y = 0; }
    if (x + w > canvas->width) w = canvas->width - x;
    if (y + h > 240) h = 240 - y;
    if (w <= 0 || h <= 0)
        return;
    u32 color = packColor(canvas, rgb);
    for (int column = x; column < x + w; column++)
    {
        // Framebuffers are turned on their side: each column of the screen is one line in memory,
        // from the bottom of the screen up.
        u8 *line = canvas->pixels + column * canvas->stride;
        for (int row = y; row < y + h; row++)
        {
            u8 *pixel = line + (239 - row) * canvas->bpp;
            switch (canvas->bpp)
            {
            case 4: *(u32 *)pixel = color; break;
            case 3: pixel[0] = color; pixel[1] = color >> 8; pixel[2] = color >> 16; break;
            default: *(u16 *)pixel = color; break;
            }
        }
    }
}

extern const u8 default_font_bin[];

// Draws text in libctru's 8x8 console font, scaled 1x. Returns the width drawn.
int canvas_text(const Canvas *canvas, int x, int y, const char *text, u32 rgb)
{
    int start = x;
    u32 color = packColor(canvas, rgb);
    for (; *text; text++, x += 8)
    {
        const u8 *glyph = default_font_bin + ((u8)*text & 0x7F) * 8;
        for (int gy = 0; gy < 8; gy++)
        {
            int row = y + gy;
            if (row < 0 || row >= 240)
                continue;
            for (int gx = 0; gx < 8; gx++)
            {
                int column = x + gx;
                if (!(glyph[gy] & (0x80 >> gx)) || column < 0 || column >= canvas->width)
                    continue;
                u8 *pixel = canvas->pixels + column * canvas->stride + (239 - row) * canvas->bpp;
                switch (canvas->bpp)
                {
                case 4: *(u32 *)pixel = color; break;
                case 3: pixel[0] = color; pixel[1] = color >> 8; pixel[2] = color >> 16; break;
                default: *(u16 *)pixel = color; break;
                }
            }
        }
    }
    return x - start;
}

// ---------------------------------------------------------------------------------------------
// Speedrun timer. It counts time while the game shows new frames. When the top screen keeps the
// same frame for longer than LOAD_GAP, the game is loading, and that stretch is left out.

#define TICKS_PER_MS 268111ULL
#define LOAD_GAP (TICKS_PER_MS * 100)

static u64 timerTicks;
static u64 lastTopSwap;
// Whether a save is being played, checked twice a second. The timer counts and shows only then.
static bool inSave;
static u64 inSaveCheckedAt;

static void timerFrame(u64 now)
{
    if (now - inSaveCheckedAt > TICKS_PER_MS * 500)
    {
        inSave = map_in_save();
        inSaveCheckedAt = now;
    }
    if (lastTopSwap && timerRunning && inSave)
    {
        u64 gap = now - lastTopSwap;
        if (gap < LOAD_GAP)
            timerTicks += gap;
#if defined(PLUGIN_TEST) || defined(PLUGIN_LOG)
        else
            overlay_log("load: %llu ms left out", gap / TICKS_PER_MS);
#endif
    }
    lastTopSwap = now;
}

static void drawTimer(const Canvas *canvas)
{
    u64 ms = timerTicks / TICKS_PER_MS;
    u32 hours = ms / 3600000, minutes = ms / 60000 % 60, seconds = ms / 1000 % 60, tenths = ms / 100 % 10;
    char text[24];
    if (hours)
        snprintf(text, sizeof(text), "%lu:%02lu:%02lu.%lu", hours, minutes, seconds, tenths);
    else
        snprintf(text, sizeof(text), "%lu:%02lu.%lu", minutes, seconds, tenths);
    int width = strlen(text) * 8;
    canvas_fill(canvas, 0, 0, width + 6, 12, 0x101010);
    canvas_text(canvas, 3, 2, text, timerRunning ? 0xFFFFFF : 0xFFD040);
}

// ---------------------------------------------------------------------------------------------
// Buttons.

static bool mapOpen, mapJustOpened, mapSwitched;
static u64 timerHeldSince, mapHeldSince;

static void runAction(u32 action, bool pressed, bool held, u64 now)
{
    switch (action)
    {
    case ACTION_MAP:
        // A press opens the map and the next one closes it. Holding the button while the map is
        // open shows the other world.
        if (pressed)
        {
            mapJustOpened = !mapOpen;
            if (!mapOpen)
            {
                map_open();
                mapOpen = true;
            }
            mapHeldSince = now;
            mapSwitched = false;
        }
        if (held && mapOpen && mapHeldSince && !mapSwitched && now - mapHeldSince > TICKS_PER_MS * 500)
        {
            map_switch_world();
            mapSwitched = true;
        }
        if (!held && mapHeldSince)
        {
            if (!mapJustOpened && !mapSwitched)
                mapOpen = false;
            mapHeldSince = 0;
        }
        break;
    case ACTION_TIMER:
        if (!settings.timer)
            break;
        if (pressed)
        {
            timerRunning = !timerRunning;
            timerHeldSince = now;
        }
        // Holding the button for two seconds resets the timer, stopped at zero.
        if (held && timerHeldSince && now - timerHeldSince > TICKS_PER_MS * 2000)
        {
            timerTicks = 0;
            timerRunning = false;
            timerHeldSince = 0;
        }
        if (!held)
            timerHeldSince = 0;
        break;
    }
}

// ---------------------------------------------------------------------------------------------

// One screen as the plugin follows it. The game shows its buffers in turn (ALBW uses three), so
// the buffer shown after this one is the one shown `count` swaps ago.
typedef struct
{
    bool top;
    int width;
    u32 learned[4]; // the game's own buffers, so the plugin never draws over the HOME Menu's
    u32 rightOf[4]; // the top screen's right-eye buffer that goes with each
    u32 history[4]; // the buffers shown last, newest first
    u64 swappedAt;
    u32 drawnVersion[4]; // the overlay version on each buffer
    u32 rightVersion[4];
    Framebuffer fb;
    bool failLogged, drawLogged, readLogged;
} Screen;

static int learnedIndex(Screen *screen, u32 address, bool learning)
{
    for (int i = 0; i < 4; i++)
        if (screen->learned[i] == address)
            return i;
    if (!learning)
        return -1;
    for (int i = 0; i < 4; i++)
        if (!screen->learned[i])
        {
            screen->learned[i] = address;
            return i;
        }
    return -1;
}

// A near-black pixel in the top left corner marks a buffer the overlay is drawn on. When the game
// puts a new frame in a buffer, the mark is gone, and the plugin draws the overlay again.
#define MARK_RGB 0x080408

static bool marked(const Canvas *canvas)
{
    u32 color = packColor(canvas, MARK_RGB);
    const u8 *pixel = canvas->pixels + 239 * canvas->bpp;
    for (u32 i = 0; i < canvas->bpp; i++)
        if (pixel[i] != (u8)(color >> (8 * i)))
            return false;
    return true;
}

// Goes up whenever what the overlay shows changes, so buffers that still have the old one get it
// again even when the game has not replaced their frame.
u32 overlayVersion = 1;

// For the first seconds, the top screen says that the plugin runs, so players can tell a plugin
// that never loaded from one that loaded and shows nothing yet.
static bool showHello = true;

// Draws the overlay on one buffer unless this version is there already.
static void drawOn(Screen *screen, u32 address, u32 *version)
{
    Canvas canvas;
    if (!canvasFor(&screen->fb, address, screen->width, &canvas))
    {
        if (!screen->failLogged)
        {
            screen->failLogged = true;
            overlay_log("%s screen: no access to buffer %08lx (format %08lx, stride %lu)", screen->top ? "top" : "bottom",
                        address, screen->fb.format, screen->fb.stride);
        }
        return;
    }
    if (*version == overlayVersion && marked(&canvas))
        return;
    if (!screen->drawLogged)
    {
        screen->drawLogged = true;
        overlay_log("%s screen: drawing on %08lx at %p (format %08lx)", screen->top ? "top" : "bottom", address,
                    canvas.pixels, screen->fb.format);
    }
    *version = overlayVersion;
    if (screen->top)
    {
        if (settings.timer && inSave)
            drawTimer(&canvas);
        if (showHello)
        {
            static const char hello[] = "ALBW Randomizer QoL plugin on";
            canvas_fill(&canvas, 0, 226, sizeof(hello) * 8 + 4, 14, 0x101010);
            canvas_text(&canvas, 3, 229, hello, 0xFFFFFF);
        }
        const char *banner = rpc_banner();
        if (banner)
        {
            canvas_fill(&canvas, 0, 14, strlen(banner) * 8 + 6, 12, 0x101060);
            canvas_text(&canvas, 3, 16, banner, 0xFFFFFF);
        }
        if (mapOpen)
            map_draw_dungeons(&canvas);
    }
    else
        map_draw(&canvas);
    canvas_fill(&canvas, 0, 0, 1, 1, MARK_RGB);
    // Without this, pixels left in the cache reach memory later, over a newer frame, and a later
    // look at the mark could see the cache's copy instead of the game's new frame. Asking the GPU
    // service to do this instead froze the game in Azahar.
    svcFlushProcessDataCache(CUR_PROCESS_HANDLE, (u32)canvas.pixels, canvas.stride * canvas.width);
}

static void followScreen(Screen *screen, u64 now, bool learning, bool wanted)
{
    if (!readFramebuffer(screen->top, &screen->fb))
    {
        if (!screen->readLogged)
        {
            screen->readLogged = true;
            overlay_log("%s screen: could not read the GPU registers", screen->top ? "top" : "bottom");
        }
        return;
    }
    u32 shown = screen->fb.shown;
    if (shown != screen->history[0])
    {
        memmove(screen->history + 1, screen->history, sizeof(u32) * 3);
        screen->history[0] = shown;
        screen->swappedAt = now;
        int index = learnedIndex(screen, shown, learning);
        if (index >= 0 && screen->top)
            screen->rightOf[index] = screen->fb.right[screen->fb.select & 1];
        if (screen->top)
            timerFrame(now);
    }
    if (!wanted)
        return;
    // Every one of the game's buffers gets the overlay as soon as the game has put a new frame in
    // it: the one on screen now, and the ones it shows next. Checking the mark costs one read.
    for (int i = 0; i < 4 && screen->learned[i]; i++)
    {
        drawOn(screen, screen->learned[i], &screen->drawnVersion[i]);
        u32 right = screen->rightOf[i];
        if (right && right != screen->learned[i])
            drawOn(screen, right, &screen->rightVersion[i]);
    }
}

static void overlayThread(void *arg)
{
    (void)arg;
    // Let the game set up its own services first.
    svcSleepThread(3000000000LL);
    extern void __libctru_init(void *);
    __libctru_init(NULL);
    srvInit();
    fsInit();
    archiveMountSdmc();
    mkdir("sdmc:/3ds", 0777);
    mkdir("sdmc:/3ds/albw-rando", 0777);
    logFile = fopen(LOG_FILE, "w");
    overlay_log("ALBW Randomizer QoL plugin, thread started (%s)", launchedByApp ? "from the app" : "loaded by Luma");
    for (int i = 0; i < 4 && threadResult[i]; i++)
        overlay_log("thread try %d refused: %08lx", i, threadResult[i]);
    Result gsp = srvGetServiceHandle(&gspHandle, "gsp::Gpu");
    overlay_log("gsp::Gpu %08lx", gsp);
    if (!launchedByApp)
    {
        readSavedSettings();
        overlay_log("saved settings read from %s", SAVED_SETTINGS);
    }
    // Archipelago games get the network server the Archipelago client talks to. Their seeds have no
    // logic map, as the multiworld's logic is not on the console.
    bool archipelago = !memcmp((const void *)AP_HEADER_LOCATION, "ARCH", 4);
#ifdef FORCE_ARCHIPELAGO
    archipelago = true;
#endif
    overlay_log("heap %lu KB, archipelago %d", HEADER->heapSize / 1024, archipelago);
    if (archipelago)
        rpc_start();
    // Loaded by Luma for every launch, the plugin stays out of the way of unpatched games and of
    // mods that are not this app's seeds.
    if (!launchedByApp && !archipelago && access(MAP_FILE, F_OK) != 0)
    {
        overlay_log("no seed from the app; plugin idle");
        return;
    }
    if (!archipelago)
        map_load();
    startInput();
    overlay_log("settings: timer %lu, zl %lu, zr %lu", settings.timer, settings.zl, settings.zr);

    static Screen top = {.top = true, .width = 400}, bottom = {.top = false, .width = 320};
    u32 lastKeys = 0;
    u64 started = svcGetSystemTick();
    for (;;)
    {
        u64 now = svcGetSystemTick();
        // The game's buffers are learned while it starts up, before the HOME Menu can open.
        bool learning = now - started < TICKS_PER_MS * 20000;
        u32 keys = readKeys();
        u32 down = keys & ~lastKeys;
        // The Old 3DS has no ZL or ZR, so SELECT with L or R stands in for them.
        bool select = keys & KEY_SELECT;
        bool zlHeld = (keys & KEY_ZL) || (select && (keys & KEY_L)), zrHeld = (keys & KEY_ZR) || (select && (keys & KEY_R));
        bool zlDown = (down & KEY_ZL) || (select && (down & KEY_L)), zrDown = (down & KEY_ZR) || (select && (down & KEY_R));
        runAction(settings.zl, zlDown, zlHeld, now);
        runAction(settings.zr, zrDown, zrHeld, now);
#if defined(PLUGIN_TEST) || defined(PLUGIN_LOG)
        if (keys != lastKeys)
            overlay_log("keys %08lx", keys);
#endif
        lastKeys = keys;
        int touchX, touchY;
        static bool wasTouched;
        bool touched = readTouch(&touchX, &touchY);
        if (touched && !wasTouched && mapOpen)
            map_touch(touchX, touchY);
        wasTouched = touched;
        static u64 lastTenth;
        if (settings.timer && timerTicks / (TICKS_PER_MS * 100) != lastTenth)
        {
            lastTenth = timerTicks / (TICKS_PER_MS * 100);
            overlayVersion++;
        }
        static bool wasOpen;
        if (mapOpen != wasOpen)
        {
            wasOpen = mapOpen;
            overlayVersion++;
        }

#if defined(PLUGIN_TEST) && !defined(REAL_INPUT)
        // Tests can't press buttons, so the test build opens the map, then turns it.
        static int step;
        if (step == 0 && now - started > TICKS_PER_MS * 20000) { extern void map_test_crack(void); map_test_crack(); map_open(); mapOpen = true; step = 1; }
        if (step == 3 && now - started > TICKS_PER_MS * 35000) { map_switch_world(); step = 2; }
        // Save what the screens show, for the test to look at.
        if ((step == 1 || step == 2) && now - started > TICKS_PER_MS * (step == 1 ? 30000 : 45000))
        {
            const char *names[2][2] = {{"sdmc:/3ds/albw-rando/test-top-1.raw", "sdmc:/3ds/albw-rando/test-bottom-1.raw"},
                                       {"sdmc:/3ds/albw-rando/test-top-2.raw", "sdmc:/3ds/albw-rando/test-bottom-2.raw"}};
            Screen *screens[2] = {&top, &bottom};
            for (int i = 0; i < 2; i++)
            {
                Canvas canvas;
                if (canvasFor(&screens[i]->fb, screens[i]->fb.shown, screens[i]->width, &canvas))
                {
                    if (i == 1)
                    {
                        extern void map_dump(const char *path);
                        map_dump(step == 1 ? "sdmc:/3ds/albw-rando/test-map-1.raw" : "sdmc:/3ds/albw-rando/test-map-2.raw");
                    }
                    FILE *file = fopen(names[step - 1][i], "wb");
                    if (file)
                    {
                        fwrite(canvas.pixels, 1, canvas.stride * canvas.width, file);
                        fclose(file);
                    }
                }
            }
            step += 2;
        }
#endif
#if defined(PLUGIN_TEST) && defined(KEEP_HEALTH)
        // Test builds can keep Link's health full, so long walks in tests survive enemies.
        {
            u32 player = *(vu32 *)0x70fb60 ? *(vu32 *)(*(vu32 *)0x70fb60 + 0x10) : 0;
            if (player && *(vu8 *)(player + 0x598) < 12)
                *(vu8 *)(player + 0x598) = 12;
        }
#endif
        rpc_poll();
        map_tick(now, mapOpen);
        // Each look at a screen asks the GPU service, so the plugin only follows the screens it
        // draws on, and the bottom one while learning the game's buffers.
        // Until the Archipelago client connects, the top screen shows the command to type into it.
        if (showHello && now - started > TICKS_PER_MS * 15000)
        {
            showHello = false;
            overlayVersion++;
        }
        bool banner = rpc_banner() != NULL || showHello;
        static bool hadBanner;
        if (banner != hadBanner)
        {
            hadBanner = banner;
            overlayVersion++;
        }
        if (settings.timer || mapOpen || learning || banner)
            followScreen(&top, now, learning, settings.timer || mapOpen || banner);
        if (mapOpen || learning)
            followScreen(&bottom, now, learning, mapOpen);
        svcSleepThread(2000000LL);
    }
}

static u8 threadStack[0x8000] __attribute__((aligned(8)));

void overlay_start(void)
{
    const u32 *config = HEADER->config;
#ifdef PLUGIN_TEST
    settings.timer = 1;
    settings.zl = ACTION_MAP;
    settings.zr = ACTION_TIMER;
    launchedByApp = true;
#else
    // Started from the app's Speed and QoL page, the choices come in the plugin's header. Loaded by
    // Luma for every launch, the plugin reads the choices the app saved instead.
    if (config[0] == REQUEST_MAGIC && config[1] == REQUEST_VERSION)
    {
        settings.timer = config[2];
        settings.zl = config[3];
        settings.zr = config[4];
        launchedByApp = true;
        settingsChosen();
    }
#endif
    (void)config;
    // If the game's limits refuse a thread with these settings, other ones are tried.
    static const struct { s32 priority, processor; } tries[] = {{0x25, -2}, {0x30, -2}, {0x3F, 0}, {0x3F, 1}};
    Handle thread;
    for (u32 i = 0; i < sizeof(tries) / sizeof(tries[0]); i++)
    {
        threadResult[i] = svcCreateThread(&thread, overlayThread, 0, (u32 *)(threadStack + sizeof(threadStack)), tries[i].priority, tries[i].processor);
        if (R_SUCCEEDED(threadResult[i]))
            break;
    }
}

// Game files plugin for the ALBW Randomizer app.
//
// The 3DS does not let one app read another app's installed game data, but a game can always read
// its own. The app asks Luma's plugin loader to load this plugin into A Link Between Worlds once,
// then starts the game. Before the game's code runs, this plugin copies the files the randomizer
// needs from the game's RomFS to the SD card, saves the game's extended header, and jumps back to
// the app. The files are the user's own game data and stay on their SD card.
//
// When Luma loads this plugin without the app asking for it, it does nothing and the game starts.

#include <3ds.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

// The app's request, in the plugin header's config words (see plugin_request in the app).
#define REQUEST_MAGIC 0x57424C41 // "ALBW"
#define REQUEST_VERSION 1

typedef struct
{
    u32 magic;
    u32 version;
    u32 returnTidLow;  // The title to jump back to, or 0 to go to the HOME Menu.
    u32 returnTidHigh;
    u32 returnMedia;
} Request;

#define PLUGIN_HEADER 0x07000000
#define CONFIG_OFFSET 0x80

#define GAME_TID 0x00040000000EC300ULL
#define OUT_DIR "sdmc:/3ds/albw-rando/game"
#define TMP_DIR "sdmc:/3ds/albw-rando/game-tmp"
#define OLD_DIR "sdmc:/3ds/albw-rando/game-old"

// Folders the randomizer never reads, left out to save time and space: the music, and the French
// and Spanish text. The sound archive stays, as the app plays the player's voice from it.
static const char *const SKIPPED[] = {"World/Sound/stream", "US_French", "US_Spanish"};

#define BUFFER_SIZE (1024 * 1024)

static const Request *request(void)
{
    return (const Request *)(PLUGIN_HEADER + CONFIG_OFFSET);
}

// Called by crt0 before anything is set up. Only plain memory reads are safe here.
int plugin_wanted(void)
{
#ifdef PLUGIN_TEST
    return 1;
#endif
    return request()->magic == REQUEST_MAGIC && request()->version == REQUEST_VERSION;
}

static u64 totalBytes, copiedBytes;
static u32 totalFiles, copiedFiles;
static u8 *buffer;

static void show(void)
{
    gfxFlushBuffers();
    gfxSwapBuffers();
    gspWaitForVBlank();
}

static void progress(const char *path)
{
    u32 percent = totalBytes ? (u32)(copiedBytes * 100 / totalBytes) : 0;
    printf("\x1b[8;1H\x1b[2K  %lu%%  %lu of %lu files, %lu of %lu MB", percent, copiedFiles, totalFiles,
           (u32)(copiedBytes >> 20), (u32)(totalBytes >> 20));
    printf("\x1b[10;1H\x1b[2K  %.46s", path);
    show();
}

static bool skipped(const char *relative)
{
    for (size_t i = 0; i < sizeof(SKIPPED) / sizeof(SKIPPED[0]); i++)
    {
        size_t len = strlen(SKIPPED[i]);
        if (strncmp(relative, SKIPPED[i], len) == 0 && (relative[len] == '\0' || relative[len] == '/'))
            return true;
    }
    return false;
}

// Walks a RomFS folder. With copy false it only counts; with copy true it copies into `out`.
// `relative` is the path inside the RomFS, without a leading slash.
static int walk(const char *relative, bool copy)
{
    char from[512], to[512];
    snprintf(from, sizeof(from), "romfs:/%s", relative);
    DIR *dir = opendir(from);
    if (!dir)
        return -1;
    if (copy)
    {
        snprintf(to, sizeof(to), "%s/romfs/%s", TMP_DIR, relative);
        mkdir(to, 0777);
    }
    int result = 0;
    struct dirent *entry;
    while (result == 0 && (entry = readdir(dir)) != NULL)
    {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0)
            continue;
        char child[512];
        snprintf(child, sizeof(child), "%s%s%s", relative, *relative ? "/" : "", entry->d_name);
        if (skipped(child))
            continue;
        snprintf(from, sizeof(from), "romfs:/%s", child);
        struct stat info;
        if (stat(from, &info) != 0)
        {
            result = -1;
            break;
        }
        if (S_ISDIR(info.st_mode))
        {
            result = walk(child, copy);
            continue;
        }
        if (!copy)
        {
            totalBytes += info.st_size;
            totalFiles++;
            continue;
        }
        if (!aptMainLoop())
        {
            result = -2;
            break;
        }
        progress(child);
        snprintf(to, sizeof(to), "%s/romfs/%s", TMP_DIR, child);
        int in = open(from, O_RDONLY);
        int out = open(to, O_WRONLY | O_CREAT | O_TRUNC, 0666);
        if (in < 0 || out < 0)
            result = -1;
        while (result == 0)
        {
            ssize_t got = read(in, buffer, BUFFER_SIZE);
            if (got < 0)
                result = -1;
            if (got <= 0)
                break;
            if (write(out, buffer, got) != got)
                result = -1;
            copiedBytes += got;
        }
        if (in >= 0)
            close(in);
        if (out >= 0)
            close(out);
        copiedFiles++;
    }
    closedir(dir);
    return result;
}

// Removes a folder and everything in it. Missing folders are fine.
static void removeTree(const char *path)
{
    DIR *dir = opendir(path);
    if (!dir)
        return;
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL)
    {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0)
            continue;
        char child[512];
        snprintf(child, sizeof(child), "%s/%s", path, entry->d_name);
        struct stat info;
        if (stat(child, &info) == 0 && S_ISDIR(info.st_mode))
            removeTree(child);
        else
            unlink(child);
    }
    closedir(dir);
    rmdir(path);
}

// Asks Luma's loader for the extended header of the running game (custom command 0x102).
static Result readExheader(u8 *out)
{
    Handle loader;
    // Fail rather than wait when the service does not exist, as on emulators.
    srvSetBlockingPolicy(true);
    Result res = srvGetServiceHandle(&loader, "Loader");
    srvSetBlockingPolicy(false);
    if (R_FAILED(res))
        return res;
    u32 *cmdbuf = getThreadCommandBuffer();
    u32 *staticbufs = getThreadStaticBuffers();
    u32 saved[2] = {staticbufs[0], staticbufs[1]};
    staticbufs[0] = IPC_Desc_StaticBuffer(0x400, 0);
    staticbufs[1] = (u32)out;
    cmdbuf[0] = IPC_MakeHeader(0x102, 0, 0);
    res = svcSendSyncRequest(loader);
    if (R_SUCCEEDED(res))
        res = cmdbuf[1];
    staticbufs[0] = saved[0];
    staticbufs[1] = saved[1];
    svcCloseHandle(loader);
    return res;
}

static bool writeFile(const char *path, const void *data, size_t size)
{
    FILE *file = fopen(path, "wb");
    if (!file)
        return false;
    bool ok = fwrite(data, 1, size, file) == size;
    return fclose(file) == 0 && ok;
}

static void __attribute__((noreturn)) finish(bool ok)
{
    if (!ok)
    {
        printf("\x1b[14;1H  Press A to go back.");
        show();
        hidInit();
        while (aptMainLoop())
        {
            hidScanInput();
            if (hidKeysDown() & KEY_A)
                break;
            gspWaitForVBlank();
        }
        hidExit();
    }
    const Request *req = request();
    u64 tid = ((u64)req->returnTidHigh << 32) | req->returnTidLow;
    if (tid)
        aptSetChainloader(tid, req->returnMedia);
    romfsExit();
    fsExit();
    gfxExit();
    // Closes the game, then jumps to the app or goes to the HOME Menu.
    aptExit();
    srvExit();
    extern void __libctru_exit(int rc) __attribute__((noreturn));
    __libctru_exit(0);
}

static void __attribute__((noreturn)) fail(const char *message, int code)
{
    printf("\x1b[12;1H\x1b[2K  Failed: %s (%d)", message, code);
    removeTree(TMP_DIR);
    finish(false);
}

void plugin_main(void) __attribute__((noreturn));
void plugin_main(void)
{
    srvInit();
    aptInit();
    gfxInitDefault();
    consoleInit(GFX_TOP, NULL);
    printf("\x1b[2;1H  ALBW Randomizer");
    printf("\x1b[4;1H  Copying the game files the randomizer needs");
    printf("\x1b[5;1H  to the SD card. This takes a few minutes.");
    show();

    Result res = fsInit();
    if (R_FAILED(res))
        fail("could not open the file system", res);
    archiveMountSdmc();
    res = romfsInit();
    if (R_FAILED(res))
        fail("could not open the game's files", res);

    u8 exheader[0x400];
    res = readExheader(exheader);
#ifdef PLUGIN_TEST
    // Emulators have no Luma loader service, so a test build carries on without the header.
    if (R_FAILED(res))
        memset(exheader, 0, sizeof(exheader));
    else
#endif
    {
        if (R_FAILED(res))
            fail("could not read the game's header", res);
        u64 tid;
        memcpy(&tid, exheader + 0x200, sizeof(tid));
        if (tid != GAME_TID)
            fail("the header is not this game's", 0);
    }

    buffer = malloc(BUFFER_SIZE);
    if (!buffer)
        fail("out of memory", 0);

    if (walk("", false) != 0)
        fail("could not list the game's files", errno);

    removeTree(TMP_DIR);
    mkdir("sdmc:/3ds", 0777);
    mkdir("sdmc:/3ds/albw-rando", 0777);
    mkdir(TMP_DIR, 0777);
    char romfs[64];
    snprintf(romfs, sizeof(romfs), "%s/romfs", TMP_DIR);
    mkdir(romfs, 0777);

    int walked = walk("", true);
    if (walked == -2)
        fail("stopped", 0);
    if (walked != 0)
        fail("could not copy a file. Is the SD card full?", errno);
    if (!writeFile(TMP_DIR "/exheader.bin", exheader, sizeof(exheader)))
        fail("could not save the game's header", errno);
    char done[128];
    int len = snprintf(done, sizeof(done), "version %d\nfiles %lu\nbytes %llu\n", REQUEST_VERSION, copiedFiles, copiedBytes);
    if (!writeFile(TMP_DIR "/done.txt", done, len))
        fail("could not finish", errno);

    // Swap the new copy in only once it is complete.
    removeTree(OLD_DIR);
    rename(OUT_DIR, OLD_DIR);
    if (rename(TMP_DIR, OUT_DIR) != 0)
        fail("could not move the copy into place", errno);
    removeTree(OLD_DIR);

    progress("");
    printf("\x1b[12;1H\x1b[2K  Done. Going back to the randomizer.");
    show();
    svcSleepThread(1500000000LL);
    finish(true);
}

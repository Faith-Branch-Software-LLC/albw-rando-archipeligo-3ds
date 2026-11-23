// Memory access over the local network for the Archipelago client.
//
// The ALBW Archipelago client reads and writes the game's memory through a small UDP protocol on
// port 45987, the scripting protocol Citra and Azahar offer for emulated games. This file serves
// the same protocol from inside the game on a real 3DS. It was written from the protocol as the
// client (apworld/albw_3ds/Interface.py) and Citra's source describe it. LittleCube's
// albw-ap-plugin first showed that a plugin can do this on hardware.
//
// Each packet starts with four little-endian words: version, request id, request type and the
// size of the data after them. A reply repeats the first three and gives its own data size.
//
// The server only runs for games patched with an Archipelago patch, since it lets any computer on
// the network read and write the game's memory.

#include <3ds.h>
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <malloc.h>
#include <netinet/in.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>

#include "overlay.h"

// Tests in Azahar use another port, since Azahar's own server already has this one.
#ifndef RPC_PORT
#define RPC_PORT 45987
#endif
#define HEADER_SIZE 16
#define MAX_DATA 1024
#define TITLE_ID 0x00040000000EC300ULL
// The socket service needs this much memory to itself.
#define SOC_BUFFER_SIZE 0x40000

enum
{
    REQUEST_PING = 0,
    REQUEST_READ = 1,
    REQUEST_WRITE = 2,
    REQUEST_PROCESS_LIST = 3,
    REQUEST_SET_GET_PROCESS = 4,
};

static int server = -1;
// What the player types into the Archipelago client, shown until the client first asks something.
static char command[32];
static bool heard;
static u32 processId;
static u8 packet[HEADER_SIZE + MAX_DATA + 8];
static u8 reply[HEADER_SIZE + MAX_DATA];

// The socket service's memory. The kernel turns down memory from the plugin's own area for it, so
// it comes from free pages at the top of the area apps keep their heap in.
static u32 *socketMemory(void)
{
    for (u32 address = 0x0F000000; address >= 0x08000000; address -= 0x100000)
    {
        MemInfo info;
        PageInfo page;
        if (R_FAILED(svcQueryMemory(&info, &page, address)) || info.state != MEMSTATE_FREE ||
            info.base_addr + info.size < address + SOC_BUFFER_SIZE)
            continue;
        u32 out;
        Result result = svcControlMemory(&out, address, 0, SOC_BUFFER_SIZE, MEMOP_ALLOC, MEMPERM_READ | MEMPERM_WRITE);
        if (R_SUCCEEDED(result))
            return (u32 *)out;
        overlay_log("rpc: memory at %08lx refused, %08lx", address, result);
        return NULL;
    }
    return NULL;
}

bool rpc_start(void)
{
    u32 *buffer = socketMemory();
    if (!buffer)
    {
        overlay_log("rpc: no memory for the socket service");
        return false;
    }
    Result result = socInit(buffer, SOC_BUFFER_SIZE);
    if (R_FAILED(result))
    {
        overlay_log("rpc: socInit failed %08lx", result);
        return false;
    }
    server = socket(AF_INET, SOCK_DGRAM, 0);
    struct sockaddr_in address = {0};
    address.sin_family = AF_INET;
    address.sin_port = htons(RPC_PORT);
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    if (server < 0 || bind(server, (struct sockaddr *)&address, sizeof(address)) < 0)
    {
        overlay_log("rpc: socket or bind failed, errno %d", errno);
        if (server >= 0)
            closesocket(server);
        server = -1;
        socExit();
        return false;
    }
    fcntl(server, F_SETFL, fcntl(server, F_GETFL, 0) | O_NONBLOCK);
    svcGetProcessId(&processId, CUR_PROCESS_HANDLE);
    struct in_addr own = {.s_addr = gethostid()};
    overlay_log("rpc: listening on %s:%d", inet_ntoa(own), RPC_PORT);
    snprintf(command, sizeof(command), "/3ds %s", inet_ntoa(own));
    return true;
}

// True when the whole range is mapped with the permission asked for.
static bool accessible(u32 address, u32 size, u32 permission)
{
    u32 end = address + size;
    if (end < address)
        return false;
    while (address < end)
    {
        MemInfo info;
        PageInfo page;
        if (R_FAILED(svcQueryMemory(&info, &page, address)) || info.state == MEMSTATE_FREE ||
            (info.perm & permission) != permission)
            return false;
        address = info.base_addr + info.size;
    }
    return true;
}

static u32 word(const u8 *at)
{
    u32 value;
    memcpy(&value, at, 4);
    return value;
}

static void put(u8 *at, u32 value)
{
    memcpy(at, &value, 4);
}

// Answers one request. Returns the size of the reply's data.
static u32 answer(u32 type, const u8 *data, u32 size, u8 *out)
{
    switch (type)
    {
    case REQUEST_READ:
    {
        if (size < 8)
            return 0;
        u32 address = word(data), length = word(data + 4);
        if (length > MAX_DATA)
            length = MAX_DATA;
        // Memory that is not there reads as zeros rather than crashing the game.
        if (accessible(address, length, MEMPERM_READ))
            memcpy(out, (const void *)address, length);
        else
            memset(out, 0, length);
        return length;
    }
    case REQUEST_WRITE:
    {
        if (size < 8)
            return 0;
        u32 address = word(data), length = word(data + 4);
        if (length > size - 8)
            length = size - 8;
        if (accessible(address, length, MEMPERM_WRITE))
            memcpy((void *)address, data + 8, length);
        return 0;
    }
    case REQUEST_PROCESS_LIST:
    {
        // Only this game's process is offered, and only once.
        u32 start = size >= 4 ? word(data) : 0;
        if (start > 0)
        {
            put(out, 0);
            return 4;
        }
        put(out, 1);
        u64 title = TITLE_ID;
        put(out + 4, processId);
        memcpy(out + 8, &title, 8);
        memcpy(out + 16, "Zelda\0\0\0", 8);
        return 24;
    }
    case REQUEST_SET_GET_PROCESS:
        put(out, processId);
        return 4;
    default:
        return 0;
    }
}

void rpc_poll(void)
{
    if (server < 0)
        return;
    for (int i = 0; i < 16; i++)
    {
        struct sockaddr_in from;
        socklen_t fromSize = sizeof(from);
        int received = recvfrom(server, packet, sizeof(packet), 0, (struct sockaddr *)&from, &fromSize);
        if (received < HEADER_SIZE)
            return;
        heard = true;
        u32 version = word(packet), id = word(packet + 4), type = word(packet + 8), size = word(packet + 12);
        if (size > (u32)received - HEADER_SIZE)
            size = received - HEADER_SIZE;
        u32 length = answer(type, packet + HEADER_SIZE, size, reply + HEADER_SIZE);
        put(reply, version);
        put(reply + 4, id);
        put(reply + 8, type);
        put(reply + 12, length);
        sendto(server, reply, HEADER_SIZE + length, 0, (struct sockaddr *)&from, fromSize);
    }
}

const char *rpc_banner(void)
{
    return server >= 0 && !heard ? command : NULL;
}

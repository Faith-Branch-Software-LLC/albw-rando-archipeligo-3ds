// Thin wrapper over citro2d. Many citro2d functions are static inline, so Rust
// cannot link to them directly. Only functions the app needs live here.

#include <3ds.h>
#include <citro2d.h>
#include <stdbool.h>
#include <stdint.h>

static C3D_RenderTarget *targets[2];
static C2D_TextBuf text_buf;

bool shim_init(void) {
    if (!C3D_Init(C3D_DEFAULT_CMDBUF_SIZE)) return false;
    if (!C2D_Init(C2D_DEFAULT_MAX_OBJECTS)) return false;
    C2D_Prepare();
    targets[0] = C2D_CreateScreenTarget(GFX_TOP, GFX_LEFT);
    targets[1] = C2D_CreateScreenTarget(GFX_BOTTOM, GFX_LEFT);
    text_buf = C2D_TextBufNew(4096);
    return targets[0] && targets[1] && text_buf;
}

void shim_exit(void) {
    C2D_TextBufDelete(text_buf);
    C2D_Fini();
    C3D_Fini();
}

void shim_frame_begin(void) {
    C3D_FrameBegin(C3D_FRAME_SYNCDRAW);
    C2D_TextBufClear(text_buf);
}

void shim_frame_end(void) {
    C3D_FrameEnd(0);
}

// screen: 0 = top, 1 = bottom. Color bytes are ABGR, as C2D_Color32 returns.
void shim_screen_begin(int screen, uint32_t clear_color) {
    C2D_TargetClear(targets[screen], clear_color);
    C2D_SceneBegin(targets[screen]);
}

void shim_rect(float x, float y, float w, float h, uint32_t color) {
    C2D_DrawRectSolid(x, y, 0.0f, w, h, color);
}

void shim_text(const char *utf8, float x, float y, float scale, uint32_t color) {
    C2D_Text text;
    C2D_TextParse(&text, text_buf, utf8);
    C2D_TextOptimize(&text);
    C2D_DrawText(&text, C2D_WithColor, x, y, 0.0f, scale, scale, color);
}

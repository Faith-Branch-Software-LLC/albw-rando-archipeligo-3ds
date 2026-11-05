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

void shim_rect_gradient(float x, float y, float w, float h, uint32_t top, uint32_t bottom) {
    C2D_DrawRectangle(x, y, 0.0f, w, h, top, top, bottom, bottom);
}

void shim_circle(float x, float y, float radius, uint32_t color) {
    C2D_DrawCircleSolid(x, y, 0.0f, radius, color);
}

void shim_triangle(float x0, float y0, float x1, float y1, float x2, float y2, uint32_t color) {
    C2D_DrawTriangle(x0, y0, color, x1, y1, color, x2, y2, color, 0.0f);
}

// align: 0 = left, 1 = center on x, 2 = right edge at x.
void shim_text_aligned(const char *utf8, float x, float y, float scale, uint32_t color, int align) {
    C2D_Text text;
    C2D_TextParse(&text, text_buf, utf8);
    C2D_TextOptimize(&text);
    u32 flags = C2D_WithColor;
    if (align == 1) flags |= C2D_AlignCenter;
    if (align == 2) flags |= C2D_AlignRight;
    C2D_DrawText(&text, flags, x, y, 0.0f, scale, scale, color);
}

void shim_text(const char *utf8, float x, float y, float scale, uint32_t color) {
    C2D_Text text;
    C2D_TextParse(&text, text_buf, utf8);
    C2D_TextOptimize(&text);
    C2D_DrawText(&text, C2D_WithColor, x, y, 0.0f, scale, scale, color);
}

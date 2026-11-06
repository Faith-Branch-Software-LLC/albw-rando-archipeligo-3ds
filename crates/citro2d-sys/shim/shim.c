// Thin wrapper over citro2d. Many citro2d functions are static inline, so Rust
// cannot link to them directly. Only functions the app needs live here.

#include <3ds.h>
#include <citro2d.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static C3D_RenderTarget *targets[2];
static C2D_TextBuf text_buf;
// Fonts loaded from the SD card, by slot. A missing font is NULL, which draws with the system font.
static C2D_Font fonts[2];

// Loads a .bcfnt font from memory into a slot, replacing any font already there. citro2d copies the
// data, so it may be read-only. Call it between frames, since text drawn this frame uses the old font.
bool shim_font_load_mem(int slot, const void *data, size_t size) {
    if (slot < 0 || slot > 1) return false;
    if (fonts[slot]) C2D_FontFree(fonts[slot]);
    fonts[slot] = C2D_FontLoadFromMem(data, size);
    return fonts[slot] != NULL;
}

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
    for (int slot = 0; slot < 2; slot++) {
        if (fonts[slot]) C2D_FontFree(fonts[slot]);
    }
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
static C2D_Font font_in(int font) {
    return font >= 0 && font <= 1 ? fonts[font] : NULL;
}


// The width that shim_text_aligned would draw the text at.
float shim_text_width(const char *utf8, float scale, int font) {
    C2D_Text text;
    C2D_TextFontParse(&text, font_in(font), text_buf, utf8);
    float width = 0.0f;
    // citro2d sizes every font so that scale 1 draws 30 pixel tall glyph cells.
    C2D_TextGetDimensions(&text, scale, scale, &width, NULL);
    return width;
}

// The height of one line of text at a scale, in pixels.
float shim_line_height(float scale) {
    return scale * C2D_FontGetInfo(NULL)->lineFeed;
}

// font: a slot from shim_font_load_mem.
void shim_text_aligned(const char *utf8, float x, float y, float scale, uint32_t color, int align, int font) {
    C2D_Text text;
    C2D_TextFontParse(&text, font_in(font), text_buf, utf8);
    C2D_TextOptimize(&text);
    u32 flags = C2D_WithColor;
    if (align == 1) flags |= C2D_AlignCenter;
    if (align == 2) flags |= C2D_AlignRight;
    C2D_DrawText(&text, flags, x, y, 0.0f, scale, scale, color);
}

typedef struct {
    C3D_Tex tex;
    Tex3DS_SubTexture sub;
    float width;
    float height;
    // Quarter turns counterclockwise to draw it with.
    int turns;
} ShimImage;

// Makes a texture from a game image already in the GPU's layout. format is a GPU_TEXCOLOR value,
// and size is the pixel data's length in bytes. Images last until the app exits.
void *shim_image_load_game(int format, int texture_width, int texture_height, int width, int height, bool transposed,
                           const u8 *pixels, size_t size) {
    ShimImage *image = malloc(sizeof(ShimImage));
    if (!image) return NULL;
    if (!C3D_TexInit(&image->tex, texture_width, texture_height, (GPU_TEXCOLOR)format)) {
        free(image);
        return NULL;
    }
    if (size > image->tex.size) size = image->tex.size;
    memcpy(image->tex.data, pixels, size);
    C3D_TexFlush(&image->tex);
    C3D_TexSetFilter(&image->tex, GPU_LINEAR, GPU_LINEAR);
    C3D_TexSetWrap(&image->tex, GPU_CLAMP_TO_EDGE, GPU_CLAMP_TO_EDGE);
    // The picture starts at the first memory row, and v = 1 is that row.
    if (transposed) {
        // Each column of the picture is a row in memory. Setting top below bottom tells citro2d the
        // art is turned, so screen x runs along v and screen y along u.
        image->sub = (Tex3DS_SubTexture){
            .width = width,
            .height = height,
            .left = 1.0f,
            .top = 0.0f,
            .right = 1.0f - (float)width / texture_height,
            .bottom = (float)height / texture_width,
        };
    } else {
        // Memory holds the picture a quarter turn clockwise, so it is drawn turned back.
        image->sub = (Tex3DS_SubTexture){
            .width = width,
            .height = height,
            .left = 0.0f,
            .top = 1.0f,
            .right = (float)width / texture_width,
            .bottom = 1.0f - (float)height / texture_height,
        };
    }
    image->width = width;
    image->height = height;
    image->turns = transposed ? 0 : 1;
    return image;
}

// Makes a texture from RGBA8 pixels already in the GPU's tiled layout, with the picture's top row
// first in memory. v = 1 is that first row, so top is 1 here.
void *shim_image_load_rgba8(int texture_width, int texture_height, int width, int height, const u8 *pixels) {
    ShimImage *image = malloc(sizeof(ShimImage));
    if (!image) return NULL;
    if (!C3D_TexInit(&image->tex, texture_width, texture_height, GPU_RGBA8)) {
        free(image);
        return NULL;
    }
    memcpy(image->tex.data, pixels, texture_width * texture_height * 4);
    C3D_TexFlush(&image->tex);
    C3D_TexSetFilter(&image->tex, GPU_LINEAR, GPU_LINEAR);
    C3D_TexSetWrap(&image->tex, GPU_CLAMP_TO_EDGE, GPU_CLAMP_TO_EDGE);
    image->sub = (Tex3DS_SubTexture){
        .width = width,
        .height = height,
        .left = 0.0f,
        .top = 1.0f,
        .right = (float)width / texture_width,
        .bottom = 1.0f - (float)height / texture_height,
    };
    image->width = width;
    image->height = height;
    image->turns = 0;
    return image;
}

void shim_image_draw(void *handle, float x, float y, float size) {
    ShimImage *image = handle;
    C2D_Image c2d = { .tex = &image->tex, .subtex = &image->sub };
    float longest = image->width > image->height ? image->width : image->height;
    float scale = size / longest;
    C2D_DrawImageAt(c2d, x, y, 0.0f, NULL, scale, scale);
}

// Draws an image scaled to fit a size by size square, centered in it, turned the way it was loaded
// plus extra_turns quarter turns counterclockwise.
void shim_image_draw_centered(void *handle, float x, float y, float size, int extra_turns) {
    ShimImage *image = handle;
    C2D_Image c2d = { .tex = &image->tex, .subtex = &image->sub };
    float longest = image->width > image->height ? image->width : image->height;
    float scale = size / longest;
    // The screen's y axis points down, so a negative angle turns counterclockwise.
    float angle = -(float)(image->turns + extra_turns) * C3D_Angle(0.25f);
    C2D_DrawImageAtRotated(c2d, x + size / 2.0f, y + size / 2.0f, 0.0f, angle, NULL, scale, scale);
}

void shim_text(const char *utf8, float x, float y, float scale, uint32_t color) {
    C2D_Text text;
    C2D_TextParse(&text, text_buf, utf8);
    C2D_TextOptimize(&text);
    C2D_DrawText(&text, C2D_WithColor, x, y, 0.0f, scale, scale, color);
}

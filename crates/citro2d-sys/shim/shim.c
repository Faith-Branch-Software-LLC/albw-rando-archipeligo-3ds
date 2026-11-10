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

// Draws the part of an image at sx, sy, sw by sh pixels, stretched to fill w by h at x, y. Used to
// stretch art to any size while keeping its edges sharp.
void shim_image_draw_part(void *handle, float sx, float sy, float sw, float sh, float x, float y, float w, float h) {
    ShimImage *image = handle;
    if (sw <= 0.0f || sh <= 0.0f || w <= 0.0f || h <= 0.0f) return;
    const Tex3DS_SubTexture *whole = &image->sub;
    float u_per_pixel = (whole->right - whole->left) / image->width;
    float v_per_pixel = (whole->bottom - whole->top) / image->height;
    Tex3DS_SubTexture part = {
        .width = (u16)sw,
        .height = (u16)sh,
        .left = whole->left + sx * u_per_pixel,
        .right = whole->left + (sx + sw) * u_per_pixel,
        .top = whole->top + sy * v_per_pixel,
        .bottom = whole->top + (sy + sh) * v_per_pixel,
    };
    C2D_Image c2d = { .tex = &image->tex, .subtex = &part };
    C2D_DrawImageAt(c2d, x, y, 0.0f, NULL, w / sw, h / sh);
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

// ---- Model previews ----
// A model is one vertex buffer of position (3 floats) and texture coordinate (2 floats), split
// into meshes that each use one texture. It is drawn over the 2D scene with its own shader, then
// citro2d's state is put back.

#define MODEL_MAX_TEXTURES 48
#define MODEL_MAX_MESHES 64

typedef struct {
    int first;
    int count;
    int texture;
    int alpha_cutoff;
    GPU_TEXTURE_WRAP_PARAM wrap_u, wrap_v;
} ShimMesh;

typedef struct {
    float *vertices;
    int vertex_count;
    C3D_Tex textures[MODEL_MAX_TEXTURES];
    int texture_count;
    ShimMesh meshes[MODEL_MAX_MESHES];
    int mesh_count;
} ShimModel;

static DVLB_s *model_dvlb;
static shaderProgram_s model_program;
static int model_projection_at;
static int model_view_at;

// shbin must stay in memory for as long as the app runs.
bool shim_model_init(const void *shbin, u32 size) {
    model_dvlb = DVLB_ParseFile((u32 *)shbin, size);
    if (!model_dvlb) return false;
    shaderProgramInit(&model_program);
    shaderProgramSetVsh(&model_program, &model_dvlb->DVLE[0]);
    model_projection_at = shaderInstanceGetUniformLocation(model_program.vertexShader, "projection");
    model_view_at = shaderInstanceGetUniformLocation(model_program.vertexShader, "modelView");
    return true;
}

// vertices holds vertex_count vertices of 5 floats. Returns NULL if memory is short.
void *shim_model_create(const float *vertices, int vertex_count) {
    ShimModel *model = calloc(1, sizeof(ShimModel));
    if (!model) return NULL;
    size_t size = (size_t)vertex_count * 5 * sizeof(float);
    model->vertices = linearAlloc(size);
    if (!model->vertices) {
        free(model);
        return NULL;
    }
    memcpy(model->vertices, vertices, size);
    GSPGPU_FlushDataCache(model->vertices, size);
    model->vertex_count = vertex_count;
    return model;
}

// Adds a texture already in the GPU's layout. Returns its index, or -1.
int shim_model_add_texture(void *handle, int format, int width, int height, const u8 *pixels, size_t size) {
    ShimModel *model = handle;
    if (model->texture_count >= MODEL_MAX_TEXTURES) return -1;
    C3D_Tex *tex = &model->textures[model->texture_count];
    if (!C3D_TexInit(tex, width, height, (GPU_TEXCOLOR)format)) return -1;
    if (size > tex->size) size = tex->size;
    memcpy(tex->data, pixels, size);
    C3D_TexFlush(tex);
    C3D_TexSetFilter(tex, GPU_LINEAR, GPU_LINEAR);
    C3D_TexSetWrap(tex, GPU_REPEAT, GPU_REPEAT);
    return model->texture_count++;
}

// texture is an index from shim_model_add_texture or -1 for none; alpha_cutoff is 0 to 255 or -1.
// wrap_u and wrap_v are GPU_TEXTURE_WRAP_PARAM values. They belong to the mesh's material, and
// two materials can share a texture, so they are set on the texture as each mesh is drawn.
bool shim_model_add_mesh(void *handle, int first, int count, int texture, int alpha_cutoff, int wrap_u, int wrap_v) {
    ShimModel *model = handle;
    if (model->mesh_count >= MODEL_MAX_MESHES) return false;
    model->meshes[model->mesh_count++] = (ShimMesh){ first, count, texture, alpha_cutoff, (GPU_TEXTURE_WRAP_PARAM)(wrap_u & 3), (GPU_TEXTURE_WRAP_PARAM)(wrap_v & 3) };
    return true;
}

// Call between frames.
void shim_model_free(void *handle) {
    ShimModel *model = handle;
    if (!model) return;
    for (int i = 0; i < model->texture_count; i++) C3D_TexDelete(&model->textures[i]);
    linearFree(model->vertices);
    free(model);
}

// Draws the model on the current 2D scene, standing with its feet-to-head height scaled to
// height pixels and centered at (x, y), turned by angle radians about its upright axis.
// center is the middle of the model's bounding box, and size its height and widest reach.
void shim_model_draw(void *handle, float x, float y, float height, float angle, float cx, float cy, float cz,
                     float model_height, float model_reach, int screen_width) {
    ShimModel *model = handle;
    if (!model || model_height <= 0.0f) return;
    C2D_Flush();

    C3D_BindProgram(&model_program);
    C3D_AttrInfo *attr = C3D_GetAttrInfo();
    AttrInfo_Init(attr);
    AttrInfo_AddLoader(attr, 0, GPU_FLOAT, 3);
    AttrInfo_AddLoader(attr, 1, GPU_FLOAT, 2);
    C3D_BufInfo *buf = C3D_GetBufInfo();
    BufInfo_Init(buf);
    BufInfo_Add(buf, model->vertices, 5 * sizeof(float), 2, 0x10);

    // The same projection citro2d uses, so x and y are screen pixels.
    C3D_Mtx projection;
    Mtx_OrthoTilt(&projection, 0.0f, (float)screen_width, 240.0f, 0.0f, 1.0f, -1.0f, true);
    float scale = height / model_height;
    // Depth stays between 0.1 and 0.9, in front of citro2d's shapes at 0.
    float depth_scale = model_reach > 0.0f ? 0.4f / model_reach : 0.0f;
    C3D_Mtx view;
    Mtx_Identity(&view);
    Mtx_Translate(&view, x, y, 0.5f, true);
    Mtx_Scale(&view, scale, -scale, depth_scale);
    Mtx_RotateY(&view, angle, true);
    Mtx_Translate(&view, -cx, -cy, -cz, true);
    C3D_FVUnifMtx4x4(GPU_VERTEX_SHADER, model_projection_at, &projection);
    C3D_FVUnifMtx4x4(GPU_VERTEX_SHADER, model_view_at, &view);

    for (int stage = 1; stage < 6; stage++) C3D_TexEnvInit(C3D_GetTexEnv(stage));
    C3D_TexEnv *env = C3D_GetTexEnv(0);
    C3D_AlphaBlend(GPU_BLEND_ADD, GPU_BLEND_ADD, GPU_ONE, GPU_ZERO, GPU_ONE, GPU_ZERO);
    C3D_DepthTest(true, GPU_GREATER, GPU_WRITE_ALL);
    C3D_CullFace(GPU_CULL_NONE);

    for (int i = 0; i < model->mesh_count; i++) {
        ShimMesh *mesh = &model->meshes[i];
        C3D_TexEnvInit(env);
        if (mesh->texture >= 0) {
            C3D_TexSetWrap(&model->textures[mesh->texture], mesh->wrap_u, mesh->wrap_v);
            C3D_TexBind(0, &model->textures[mesh->texture]);
            C3D_TexEnvSrc(env, C3D_Both, GPU_TEXTURE0, 0, 0);
        } else {
            C3D_TexEnvSrc(env, C3D_Both, GPU_CONSTANT, 0, 0);
            C3D_TexEnvColor(env, 0xFFC0C0C0);
        }
        C3D_TexEnvFunc(env, C3D_Both, GPU_REPLACE);
        if (mesh->alpha_cutoff >= 0) {
            C3D_AlphaTest(true, GPU_GEQUAL, mesh->alpha_cutoff);
        } else {
            C3D_AlphaTest(false, GPU_ALWAYS, 0);
        }
        C3D_DrawArrays(GPU_TRIANGLES, mesh->first, mesh->count);
    }

    // Puts back what citro2d expects.
    C3D_AlphaTest(false, GPU_ALWAYS, 0);
    C3D_AlphaBlend(GPU_BLEND_ADD, GPU_BLEND_ADD, GPU_SRC_ALPHA, GPU_ONE_MINUS_SRC_ALPHA, GPU_SRC_ALPHA,
                   GPU_ONE_MINUS_SRC_ALPHA);
    C2D_Prepare();
}

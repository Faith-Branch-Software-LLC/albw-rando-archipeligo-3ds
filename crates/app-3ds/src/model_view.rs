//! Shows a turning preview of Link's model on the Models and Voices page. The model comes from the
//! chosen mod, or from the player's own game when no mod is chosen. Each one is pulled out of its
//! archive once and kept in the SD card cache, since the archive takes a moment to decompress.

use std::ffi::c_void;
use std::fs;
use std::path::Path;

use ui_core::{bch, sarc};

use crate::job;

const CACHE: &str = "sdmc:/3ds/albw-rando/cache/models";
/// Link's model inside `Archive/ActorCommon.szs`.
const LINK: &str = "World/Actor/link.bch";
pub const ARCHIVE: &str = "ActorCommon.szs";

fn vanilla_cache() -> String {
    format!("{CACHE}/game.bch")
}

/// Whether the game's own model still needs to be cached.
pub fn needs_vanilla() -> bool {
    !Path::new(&vanilla_cache()).exists()
}

/// Pulls Link's model out of `ActorCommon.szs`, compressed or not.
fn link_from_archive(archive: &[u8]) -> Result<Vec<u8>, String> {
    let decompressed;
    let data = if archive.starts_with(b"Yaz0") {
        decompressed = rom::files::yaz0::decompress(archive).map_err(|err| format!("could not decompress {ARCHIVE}: {err}"))?;
        &decompressed[..]
    } else {
        archive
    };
    sarc::find_hashed(data, LINK).map(<[u8]>::to_vec).ok_or_else(|| format!("{ARCHIVE} has no {LINK}"))
}

/// Saves the game's own model from its `ActorCommon.szs`. Returns a note for the log.
pub fn cache_vanilla(archive: &[u8]) -> String {
    let result = link_from_archive(archive).and_then(|model| {
        fs::create_dir_all(CACHE).map_err(|err| err.to_string())?;
        fs::write(vanilla_cache(), model).map_err(|err| err.to_string())
    });
    match result {
        Ok(()) => "Saved the game's model for previews.".to_owned(),
        Err(err) => format!("Could not save the game's model for previews: {err}"),
    }
}

/// The model file for a mod: from the cache, or pulled from the mod's archive and cached. A mod
/// that does not change Link's model shows the game's model.
fn model_file(mod_name: Option<&str>) -> Result<Vec<u8>, String> {
    let Some(name) = mod_name else {
        return fs::read(vanilla_cache()).map_err(|_| "the game's model is not cached yet".to_owned());
    };
    let romfs = job::romfs_of(&Path::new(job::MODEL_DIR).join(name)).ok_or("the mod has no romfs folder")?;
    let archive_path = romfs.join("Archive").join(ARCHIVE);
    let Ok(metadata) = fs::metadata(&archive_path) else {
        return model_file(None);
    };
    // The archive's size is in the name, so a changed mod is read again.
    let safe: String = name.chars().map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' }).collect();
    let cached = format!("{CACHE}/{safe}-{}.bch", metadata.len());
    if let Ok(model) = fs::read(&cached) {
        return Ok(model);
    }
    let archive = fs::read(&archive_path).map_err(|err| err.to_string())?;
    let model = link_from_archive(&archive)?;
    let _ = fs::create_dir_all(CACHE);
    let _ = fs::write(&cached, &model);
    Ok(model)
}

/// A model in GPU memory, ready to draw.
pub struct Preview {
    handle: *mut c_void,
    center: [f32; 3],
    height: f32,
    reach: f32,
}

impl Preview {
    /// Loads and uploads the model for a mod, or the game's model for `None`.
    pub fn load(mod_name: Option<&str>) -> Result<Self, String> {
        let model = bch::parse(model_file(mod_name)?)?;
        let (low, high) = model.bounds().ok_or("the model has no triangles")?;
        let center = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0, (low[2] + high[2]) / 2.0];
        let reach = model
            .meshes
            .iter()
            .flat_map(|mesh| &mesh.vertices)
            .map(|vertex| (vertex.position[0] - center[0]).hypot(vertex.position[2] - center[2]))
            .fold(0.0, f32::max);

        let vertices: Vec<f32> = model.meshes.iter().flat_map(|mesh| &mesh.vertices).flat_map(|v| [v.position[0], v.position[1], v.position[2], v.uv[0], v.uv[1]]).collect();
        let count = i32::try_from(vertices.len() / 5).map_err(|_| "the model is too big")?;
        // SAFETY: `vertices` holds `count` vertices of five floats, and the shim copies them.
        let handle = unsafe { citro2d_sys::shim_model_create(vertices.as_ptr(), count) };
        if handle.is_null() {
            return Err("not enough memory for the model".to_owned());
        }
        let preview = Self { handle, center, height: high[1] - low[1], reach };
        let mut uploaded = Vec::new();
        for texture in &model.textures {
            // SAFETY: The handle is live, and the shim copies the pixels.
            let index = unsafe {
                citro2d_sys::shim_model_add_texture(
                    handle,
                    texture.gpu_format.into(),
                    texture.width.into(),
                    texture.height.into(),
                    texture.pixels.as_ptr(),
                    texture.pixels.len(),
                )
            };
            uploaded.push(index);
        }
        let mut first = 0;
        for mesh in &model.meshes {
            let material = model.materials.get(mesh.material);
            let texture = material.and_then(|material| material.texture).and_then(|index| uploaded.get(index).copied()).unwrap_or(-1);
            let cutoff = material.and_then(|material| material.alpha_cutoff).map_or(-1, i32::from);
            // Repeat when the material is missing.
            let [wrap_u, wrap_v] = material.map_or([2, 2], |material| material.wrap);
            let count = mesh.vertices.len() as i32;
            // SAFETY: The handle is live.
            unsafe { citro2d_sys::shim_model_add_mesh(handle, first, count, texture, cutoff, wrap_u.into(), wrap_v.into()) };
            first += count;
        }
        Ok(preview)
    }

    /// Draws the model `height` pixels tall, centered at (x, y) on a screen `screen_width` wide.
    pub fn draw(&self, x: f32, y: f32, height: f32, angle: f32, screen_width: i32) {
        // SAFETY: Called inside a frame, and the handle is live.
        unsafe {
            citro2d_sys::shim_model_draw(
                self.handle,
                x,
                y,
                height,
                angle,
                self.center[0],
                self.center[1],
                self.center[2],
                self.height,
                self.reach,
                screen_width,
            )
        };
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        // SAFETY: The handle came from `shim_model_create`, and previews are dropped between frames.
        unsafe { citro2d_sys::shim_model_free(self.handle) };
    }
}

/// Starts the preview shader. Returns false if it could not load.
pub fn start() -> bool {
    let shader = &citro2d_sys::MODEL_SHADER.0;
    // SAFETY: The shader is static, so it outlives every use.
    unsafe { citro2d_sys::shim_model_init(shader.as_ptr().cast(), shader.len() as u32) }
}

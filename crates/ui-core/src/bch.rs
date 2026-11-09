//! Reads a 3D model (`.bch`) well enough to show it standing still: triangle meshes in the bind
//! pose, each with its material's texture. Animations, lighting and most material settings are
//! skipped. The layout follows gdkchan's notes in Ohana3DS; only the newer layout that A Link
//! Between Worlds uses (backward compatibility 0x21 and up) is handled.

/// One vertex: position, then texture coordinate.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// Triangles drawn with one material, three vertices each.
#[derive(Debug, Default)]
pub struct Mesh {
    pub material: usize,
    pub vertices: Vec<Vertex>,
}

#[derive(Debug, Default)]
pub struct Material {
    /// Index into `Model::textures`, if the material names one that exists.
    pub texture: Option<usize>,
    /// If set, pixels whose alpha is below this (0 to 255) are not drawn.
    pub alpha_cutoff: Option<u8>,
    /// How texture coordinates outside 0 to 1 wrap, across then down, as citro3d
    /// `GPU_TEXTURE_WRAP_PARAM` values: 0 clamp to edge, 1 clamp to border, 2 repeat, 3 mirror.
    /// Most of Link's textures hold half of him and mirror it for the other side.
    pub wrap: [u8; 2],
}

/// A texture in the GPU's own layout.
#[derive(Debug, Default)]
pub struct Texture {
    pub name: String,
    pub width: u16,
    pub height: u16,
    /// The citro3d `GPU_TEXCOLOR` value. The file stores the GPU's value directly.
    pub gpu_format: u8,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct Model {
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
}

impl Model {
    /// The smallest and largest corner of a box around every vertex.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut vertices = self.meshes.iter().flat_map(|mesh| mesh.vertices.iter());
        let first = vertices.next()?.position;
        Some(vertices.fold((first, first), |(low, high), vertex| {
            let p = vertex.position;
            ([low[0].min(p[0]), low[1].min(p[1]), low[2].min(p[2])], [high[0].max(p[0]), high[1].max(p[1]), high[2].max(p[2])])
        }))
    }

    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(|mesh| mesh.vertices.len()).sum()
    }
}

type Result<T> = std::result::Result<T, String>;

/// The model file, with its offsets turned into positions in the file.
struct File {
    data: Vec<u8>,
}

impl File {
    fn u32(&self, at: usize) -> Result<u32> {
        let bytes = self.data.get(at..at + 4).ok_or_else(|| format!("model ends early, at 0x{at:X}"))?;
        Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
    }

    fn at(&self, at: usize) -> Result<usize> {
        Ok(self.u32(at)? as usize)
    }

    fn u16(&self, at: usize) -> Result<u16> {
        let bytes = self.data.get(at..at + 2).ok_or_else(|| format!("model ends early, at 0x{at:X}"))?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn f32(&self, at: usize) -> Result<f32> {
        Ok(f32::from_bits(self.u32(at)?))
    }

    fn byte(&self, at: usize) -> Result<u8> {
        self.data.get(at).copied().ok_or_else(|| format!("model ends early, at 0x{at:X}"))
    }

    fn put(&mut self, at: usize, value: u32) -> Result<()> {
        let bytes = self.data.get_mut(at..at + 4).ok_or_else(|| format!("bad relocation at 0x{at:X}"))?;
        bytes.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn add(&mut self, at: usize, base: u32) -> Result<()> {
        let value = self.u32(at)?;
        self.put(at, value.wrapping_add(base))
    }

    fn string(&self, at: usize) -> Result<String> {
        let start = self.at(at)?;
        if start == 0 {
            return Ok(String::new());
        }
        let rest = self.data.get(start..).ok_or("bad name")?;
        let end = rest.iter().position(|&byte| byte == 0).ok_or("unterminated name")?;
        Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
    }
}

/// The last value written to each GPU register by a command list, and the float uniforms.
struct Commands {
    registers: std::collections::HashMap<u16, u32>,
    uniforms: std::collections::HashMap<u32, Vec<f32>>,
}

const BLOCK_END: u16 = 0x23D;
const UNIFORM_CONFIG: u16 = 0x2C0;
const UNIFORM_DATA: u16 = 0x2C1;

impl Commands {
    fn read(file: &File, start: usize, words: usize) -> Result<Self> {
        let mut registers = std::collections::HashMap::new();
        let mut uniforms: std::collections::HashMap<u32, Vec<f32>> = std::collections::HashMap::new();
        let mut current = 0;
        let (mut at, mut read) = (start, 0);
        while read < words {
            let parameter = file.u32(at)?;
            let header = file.u32(at + 4)?;
            at += 8;
            read += 2;
            let mut id = (header & 0xFFFF) as u16;
            let mask = (header >> 16) & 0xF;
            let extra = (header >> 20) & 0x7FF;
            let consecutive = header >> 31 == 1;
            let byte_mask = (0..4).filter(|bit| mask & (1 << bit) != 0).fold(0u32, |m, bit| m | 0xFF << (8 * bit));
            let write = |registers: &mut std::collections::HashMap<u16, u32>, id: u16, value: u32| {
                let old = registers.get(&id).copied().unwrap_or(0);
                registers.insert(id, (old & !byte_mask) | (value & byte_mask));
            };
            write(&mut registers, id, parameter);
            if id == BLOCK_END {
                break;
            }
            let mut values = Vec::new();
            if id == UNIFORM_CONFIG {
                current = parameter & 0x7FFF_FFFF;
            } else if id == UNIFORM_DATA {
                values.push(f32::from_bits(registers[&id]));
            }
            for _ in 0..extra {
                if consecutive {
                    id += 1;
                }
                let value = file.u32(at)?;
                at += 4;
                read += 1;
                write(&mut registers, id, value);
                if id > UNIFORM_CONFIG && id < UNIFORM_DATA + 8 {
                    values.push(f32::from_bits(registers[&id]));
                }
            }
            if !values.is_empty() {
                uniforms.entry(current).or_default().extend(values);
            }
            // Each command starts on an 8 byte boundary.
            if at % 8 != 0 {
                at += 4;
                read += 1;
            }
        }
        Ok(Self { registers, uniforms })
    }

    fn get(&self, id: u16) -> u32 {
        self.registers.get(&id).copied().unwrap_or(0)
    }

    fn pair(&self, low: u16, high: u16) -> u64 {
        u64::from(self.get(low)) | u64::from(self.get(high)) << 32
    }
}

/// Undoes the file's relocation table, so stored offsets become positions in the file.
fn relocate(data: Vec<u8>) -> Result<(File, usize)> {
    let mut file = File { data };
    if file.data.get(..4) != Some(b"BCH\0") {
        return Err("not a BCH model".to_owned());
    }
    let compatibility = file.byte(4)?;
    if compatibility < 0x21 {
        return Err(format!("model version 0x{compatibility:X} is older than this app reads"));
    }
    // Section offsets, then their lengths: main, strings, GPU commands, data, extended data,
    // and relocations.
    let word = |index: usize| file.u32(8 + index * 4);
    let (main, strings, gpu, data, extended, relocations) = (word(0)?, word(1)?, word(2)?, word(3)?, word(4)?, word(5)?);
    let relocations_length = word(11)?;
    for entry in (relocations..relocations + relocations_length).step_by(4) {
        let value = file.u32(entry as usize)?;
        let offset = value & 0x1FF_FFFF;
        let main_at = (offset * 4 + main) as usize;
        match value >> 25 {
            0 => file.add(main_at, main)?,
            1 => file.add((offset + main) as usize, strings)?,
            2 => file.add(main_at, gpu)?,
            7 | 0xC => file.add(main_at, data)?,
            kind => {
                let gpu_at = (offset * 4 + gpu) as usize;
                let old = file.u32(gpu_at)?;
                match kind {
                    0x25 | 0x26 => file.put(gpu_at, old.wrapping_add(data))?,
                    0x27 => file.put(gpu_at, (old.wrapping_add(data) & 0x7FFF_FFFF) | 0x8000_0000)?,
                    0x28 => file.put(gpu_at, old.wrapping_add(data) & 0x7FFF_FFFF)?,
                    0x2B => file.put(gpu_at, old.wrapping_add(extended))?,
                    0x2C => file.put(gpu_at, (old.wrapping_add(extended) & 0x7FFF_FFFF) | 0x8000_0000)?,
                    0x2D => file.put(gpu_at, old.wrapping_add(extended) & 0x7FFF_FFFF)?,
                    _ => {}
                }
            }
        }
    }
    Ok((file, main as usize))
}

/// Bits per texel for each GPU texture format.
fn texture_bits(format: u8) -> Option<usize> {
    Some(match format {
        0 => 32,
        1 => 24,
        2..=6 => 16,
        7..=9 => 8,
        10 | 11 | 12 => 4,
        13 => 8,
        _ => return None,
    })
}

/// A 4 by 4 matrix for row vectors: a point `p` moves to `p * m`.
type Matrix = [[f32; 4]; 4];

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [[0.0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

/// A bone's transform relative to its parent: scale, then turns about X, Y and Z, then move.
fn bone_matrix(scale: [f32; 3], rotation: [f32; 3], translation: [f32; 3]) -> Matrix {
    let mut m = [[scale[0], 0.0, 0.0, 0.0], [0.0, scale[1], 0.0, 0.0], [0.0, 0.0, scale[2], 0.0], [0.0, 0.0, 0.0, 1.0]];
    let (s, c) = rotation[0].sin_cos();
    m = multiply(&m, &[[1.0, 0.0, 0.0, 0.0], [0.0, c, s, 0.0], [0.0, -s, c, 0.0], [0.0, 0.0, 0.0, 1.0]]);
    let (s, c) = rotation[1].sin_cos();
    m = multiply(&m, &[[c, 0.0, -s, 0.0], [0.0, 1.0, 0.0, 0.0], [s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]]);
    let (s, c) = rotation[2].sin_cos();
    m = multiply(&m, &[[c, s, 0.0, 0.0], [-s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]);
    multiply(&m, &[[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [translation[0], translation[1], translation[2], 1.0]])
}

fn transform(p: [f32; 3], m: &Matrix) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (j, value) in out.iter_mut().enumerate() {
        *value = p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + m[3][j];
    }
    out
}

/// Vertex attribute kinds the GPU knows.
const POSITION: u64 = 0;
const TEXTURE_0: u64 = 4;
const BONE_INDEX: u64 = 7;
const BONE_WEIGHT: u64 = 8;
const SMOOTH_SKINNING: u16 = 1;

/// Reads one vertex attribute of `count` values in `kind` (0 byte, 1 unsigned byte, 2 short,
/// 3 float). Returns the values and the bytes used.
fn attribute(file: &File, at: usize, kind: u64, count: usize) -> Result<([f32; 4], usize)> {
    let size = [1, 1, 2, 4][kind as usize];
    let mut values = [0.0; 4];
    for (index, value) in values.iter_mut().enumerate().take(count) {
        let here = at + index * size;
        *value = match kind {
            0 => file.byte(here)? as i8 as f32,
            1 => f32::from(file.byte(here)?),
            2 => file.u16(here)? as i16 as f32,
            _ => file.f32(here)?,
        };
    }
    Ok((values, size * count))
}

/// Reads the first model in a `.bch` file.
pub fn parse(data: Vec<u8>) -> Result<Model> {
    let (file, main) = relocate(data)?;
    let header = |index: usize| file.at(main + index * 4);
    let mut model = Model::default();

    // Textures: each has GPU commands that give its address, size and format.
    let (textures_at, texture_count) = (header(9)?, header(10)?);
    for index in 0..texture_count {
        let entry = file.at(textures_at + index * 4)?;
        let commands = Commands::read(&file, file.at(entry)?, file.at(entry + 4)?)?;
        let name = file.string(entry + 0x1C)?;
        let size = commands.get(0x82);
        let (width, height) = ((size >> 16) as u16, (size & 0xFFFF) as u16);
        let gpu_format = commands.get(0x8E) as u8;
        let start = commands.get(0x85) as usize;
        let bits = texture_bits(gpu_format).ok_or_else(|| format!("texture {name} has unknown format {gpu_format}"))?;
        let length = usize::from(width) * usize::from(height) * bits / 8;
        let pixels = file.data.get(start..start + length).ok_or_else(|| format!("texture {name} runs past the file"))?.to_vec();
        model.textures.push(Texture { name, width, height, gpu_format, pixels });
    }

    if header(1)? == 0 {
        return Err("the file has no model".to_owned());
    }
    let at = file.at(header(0)?)?;
    // After flags (4 bytes) and a 4 by 3 matrix (48 bytes) come the material and mesh tables.
    let table = at + 52;
    let (materials_at, material_count) = (file.at(table)?, file.at(table + 4)?);
    let (meshes_at, mesh_count) = (file.at(table + 12)?, file.at(table + 16)?);
    let (bones_at, bone_count) = (file.at(table + 60)?, file.at(table + 64)?);

    for index in 0..material_count {
        let entry = materials_at + index * 0x2C;
        let texture_name = file.string(entry + 0x1C)?;
        let texture = model.textures.iter().position(|texture| texture.name == texture_name);
        // The fragment commands, 200 bytes into the material's settings, hold the alpha test.
        let settings = file.at(entry)?;
        let fragment = Commands::read(&file, file.at(settings + 200)?, file.at(settings + 204)?)?;
        let alpha_test = fragment.get(0x104);
        let alpha_cutoff = (alpha_test & 1 == 1).then_some((alpha_test >> 8) as u8);
        // The first texture mapper, which the material's pointer at +24 leads to, holds the wrap
        // modes in its second and third bytes.
        let mapper = file.at(file.at(entry + 24)?)?;
        let wrap = [(mapper >> 8) as u8 & 3, (mapper >> 16) as u8 & 3];
        model.materials.push(Material { texture, alpha_cutoff, wrap });
    }

    // Bones place the parts that follow a single bone.
    let mut bones: Vec<Matrix> = Vec::new();
    for index in 0..bone_count {
        let entry = bones_at + index * 100;
        let parent = file.u16(entry + 4)? as i16;
        let vector = |at: usize| -> Result<[f32; 3]> { Ok([file.f32(at)?, file.f32(at + 4)?, file.f32(at + 8)?]) };
        let local = bone_matrix(vector(entry + 8)?, vector(entry + 20)?, vector(entry + 32)?);
        let world = match usize::try_from(parent).ok().and_then(|parent| bones.get(parent)) {
            Some(parent) => multiply(&local, parent),
            None => local,
        };
        bones.push(world);
    }

    for index in 0..mesh_count {
        let entry = meshes_at + index * 0x38;
        let material = usize::from(file.u16(entry)?);
        let flags = file.u16(entry + 2)?;
        // Silhouette meshes draw an outline, not the model.
        if flags & 1 == 1 {
            continue;
        }
        let commands = Commands::read(&file, file.at(entry + 8)?, file.at(entry + 12)?)?;
        let (faces_at, face_count) = (file.at(entry + 16)?, file.at(entry + 20)?);
        // The vertex shader's uniforms 6 and 7 hold the position offset and value scales, written
        // last value first.
        let mut offset = commands.uniforms.get(&6).cloned().unwrap_or_default();
        offset.reverse();
        let mut scales = commands.uniforms.get(&7).cloned().unwrap_or_default();
        scales.reverse();
        let scale = |index: usize| scales.get(index).copied().unwrap_or(1.0);
        let (uv_scale, position_scale) = (scale(0), scale(4));
        let offset = |index: usize| offset.get(index).copied().unwrap_or(0.0);

        let kinds = commands.pair(0x2BB, 0x2BC);
        let formats = commands.pair(0x201, 0x202);
        let buffer = commands.get(0x203) as usize;
        let layout = commands.get(0x205);
        let stride = ((layout >> 16) & 0xFF) as usize;
        let attribute_count = (layout >> 28) as usize;
        let order = u64::from(commands.get(0x204)) | u64::from(layout & 0xFFFF) << 32;

        let mut vertices = Vec::new();
        for face in 0..face_count {
            let face_at = faces_at + face * 0x34;
            let skinning = file.u16(face_at)?;
            let node_count = usize::from(file.u16(face_at + 2)?);
            let nodes = (0..node_count).map(|n| file.u16(face_at + 4 + n * 2).map(usize::from)).collect::<Result<Vec<_>>>()?;
            let indices = Commands::read(&file, file.at(face_at + 0x2C)?, file.at(face_at + 0x30)?)?;
            let index_config = indices.get(0x227);
            let short_indices = index_config >> 31 == 1;
            let indices_at = (index_config & 0x7FFF_FFFF) as usize;
            let count = indices.get(0x228) as usize;
            for corner in 0..count {
                let index = if short_indices { usize::from(file.u16(indices_at + corner * 2)?) } else { usize::from(file.byte(indices_at + corner)?) };
                let start = buffer + index * stride;
                // Bytes into the vertex. The GPU starts each attribute at a multiple of its value
                // size, so a 3-byte normal before a float is followed by one byte of padding.
                let mut at = 0;
                let (mut position, mut uv, mut bone) = ([0.0; 3], [0.0; 2], None);
                for slot_index in 0..attribute_count {
                    let slot = (order >> (slot_index * 4)) & 0xF;
                    // Slots 12 to 15 are 4 to 16 bytes of padding instead of an attribute.
                    if slot >= 12 {
                        at += (slot as usize - 11) * 4;
                        continue;
                    }
                    let kind = (kinds >> (slot * 4)) & 0xF;
                    let format = (formats >> (slot * 4)) & 0xF;
                    let value_size = [1, 1, 2, 4][(format & 3) as usize];
                    at = at.div_ceil(value_size) * value_size;
                    let value_kind = if kind == BONE_WEIGHT { 1 } else { format & 3 };
                    let (values, used) = attribute(&file, start + at, value_kind, (format >> 2) as usize + 1)?;
                    at += used;
                    match kind {
                        POSITION => position = [0, 1, 2].map(|i| values[i] * position_scale + offset(i)),
                        TEXTURE_0 => uv = [values[0] * uv_scale, values[1] * uv_scale],
                        BONE_INDEX => bone = nodes.get(values[0] as usize).copied(),
                        _ => {}
                    }
                }
                // Smooth skinned vertices are already in place. Others follow their one bone.
                if skinning != SMOOTH_SKINNING {
                    if let Some(matrix) = bone.or(nodes.first().copied()).and_then(|bone| bones.get(bone)) {
                        position = transform(position, matrix);
                    }
                }
                vertices.push(Vertex { position, uv });
            }
        }
        vertices.truncate(vertices.len() / 3 * 3);
        model.meshes.push(Mesh { material, vertices });
    }
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_other_files() {
        assert!(parse(b"not a model".to_vec()).is_err());
        let mut old = b"BCH\0\x07".to_vec();
        old.resize(64, 0);
        assert!(parse(old).unwrap_err().contains("older"));
    }

    #[test]
    fn bone_matrices_turn_and_move() {
        let quarter = std::f32::consts::FRAC_PI_2;
        let m = bone_matrix([1.0; 3], [0.0, 0.0, quarter], [5.0, 0.0, 0.0]);
        let p = transform([1.0, 0.0, 0.0], &m);
        assert!((p[0] - 5.0).abs() < 1e-5 && (p[1] - 1.0).abs() < 1e-5, "{p:?}");
    }

    #[test]
    fn reads_masked_and_repeated_commands() {
        // Writes 0x11223344 to register 0x82 with a mask on the low two bytes, then two values
        // starting at 0x2C1 after choosing uniform 7.
        let mut data = Vec::new();
        for word in [0x0000_0007u32, 0x000F_02C0, 0x3F80_0000, 0x801F_02C1, 0x4000_0000, 0, 0x1122_3344, 0x0003_0082] {
            data.extend_from_slice(&word.to_le_bytes());
        }
        let file = File { data };
        let commands = Commands::read(&file, 0, 8).unwrap();
        assert_eq!(commands.uniforms[&7], vec![1.0, 2.0]);
        assert_eq!(commands.get(0x82), 0x3344);
    }

    /// Checks a real model when `ALBW_LINK_BCH` names one, such as Link's model taken from your
    /// own game. It is never kept in the repository.
    #[test]
    fn reads_a_real_model_when_one_is_given() {
        let Ok(path) = std::env::var("ALBW_LINK_BCH") else { return };
        let model = parse(std::fs::read(path).unwrap()).unwrap();
        let (low, high) = model.bounds().unwrap();
        assert!(model.vertex_count() > 1000, "{}", model.vertex_count());
        assert!(high[1] - low[1] > 1.0, "{low:?} {high:?}");
        assert!(model.materials.iter().all(|material| material.texture.is_some()));
        assert!(model.materials.iter().any(|material| material.alpha_cutoff.is_some()));
        // Link's models mirror most textures, so a wrong read shows as repeat (2) or clamp (0).
        assert!(model.materials.iter().filter(|material| material.wrap == [3, 3]).count() >= 4);
        // Texture coordinates read from the wrong bytes come out huge or not a number.
        let uvs = model.meshes.iter().flat_map(|mesh| mesh.vertices.iter().flat_map(|vertex| vertex.uv));
        assert!(uvs.into_iter().all(|value| value.abs() < 16.0));
    }
}

//! Binary glTF 2.0 (`.glb`), written with the crate's own JSON.
//!
//! One scene, one node per model at its origin, one mesh per model with POSITION,
//! NORMAL, TEXCOORD_0 and `u32` indices. One opaque material samples the palette
//! texture, embedded as PNG, with nearest filtering and clamped edges; translucent
//! keys use a second material with `alphaMode: BLEND`. Nothing depends on the time
//! or the platform, so the same document gives the same bytes.

use std::fmt::Write as _;

use crate::json::quote;
use crate::mesh::Mesh;
use crate::{Error, ErrorCode, Result};

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const NEAREST: u32 = 9728;
const CLAMP_TO_EDGE: u32 = 33071;

/// A model to write: its name and its mesh.
pub(crate) struct Part<'a> {
    pub name: &'a str,
    pub mesh: &'a Mesh,
}

/// The shortest decimal that reads back as `v`, as JSON takes it.
fn number(v: f32) -> String {
    // Rust prints the shortest round-trip form without an exponent; `-0` is valid.
    format!("{v}")
}

fn vec3(v: [f32; 3]) -> String {
    format!("[{},{},{}]", number(v[0]), number(v[1]), number(v[2]))
}

struct Writer {
    bin: Vec<u8>,
    views: Vec<String>,
    accessors: Vec<String>,
}

impl Writer {
    fn align(&mut self) {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
    }

    /// Adds a buffer view of `bytes`, returning its index.
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        self.align();
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        let mut view = format!(
            "{{\"buffer\":0,\"byteOffset\":{offset},\"byteLength\":{}",
            bytes.len()
        );
        if let Some(t) = target {
            let _ = write!(view, ",\"target\":{t}");
        }
        view.push('}');
        self.views.push(view);
        self.views.len() - 1
    }

    fn accessor(
        &mut self,
        view: usize,
        component: u32,
        count: usize,
        kind: &str,
        extra: &str,
    ) -> usize {
        self.accessors.push(format!(
            "{{\"bufferView\":{view},\"componentType\":{component},\"count\":{count},\"type\":\"{kind}\"{extra}}}"
        ));
        self.accessors.len() - 1
    }
}

/// Writes `parts` and the palette texture `png` as one `.glb` file.
pub(crate) fn glb(parts: &[Part], png: Option<&[u8]>) -> Result<Vec<u8>> {
    let mut w = Writer {
        bin: Vec::new(),
        views: Vec::new(),
        accessors: Vec::new(),
    };
    let any_opaque = parts.iter().any(|p| p.mesh.translucent_start > 0);
    let any_clear = parts
        .iter()
        .any(|p| p.mesh.indices.len() > p.mesh.translucent_start);
    let opaque_material = any_opaque.then_some(0);
    let clear_material = any_clear.then_some(any_opaque as usize);
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    for part in parts {
        let mesh = part.mesh;
        let mut node = format!("{{\"name\":{}", quote(part.name));
        if let Some(bounds) = mesh.bounds() {
            let floats = |values: &mut dyn Iterator<Item = f32>| -> Vec<u8> {
                values.flat_map(f32::to_le_bytes).collect()
            };
            let n = mesh.positions.len();
            let pv = w.view(
                &floats(&mut mesh.positions.iter().flatten().copied()),
                Some(ARRAY_BUFFER),
            );
            let position = w.accessor(
                pv,
                FLOAT,
                n,
                "VEC3",
                &format!(",\"min\":{},\"max\":{}", vec3(bounds.min), vec3(bounds.max)),
            );
            let nv = w.view(
                &floats(&mut mesh.normals.iter().flatten().copied()),
                Some(ARRAY_BUFFER),
            );
            let normal = w.accessor(nv, FLOAT, n, "VEC3", "");
            let tv = w.view(
                &floats(&mut mesh.uvs.iter().flatten().copied()),
                Some(ARRAY_BUFFER),
            );
            let uv = w.accessor(tv, FLOAT, n, "VEC2", "");
            let attributes =
                format!("{{\"POSITION\":{position},\"NORMAL\":{normal},\"TEXCOORD_0\":{uv}}}");
            let mut primitives = Vec::new();
            for (indices, material) in [
                (mesh.opaque_indices(), opaque_material),
                (mesh.translucent_indices(), clear_material),
            ] {
                if indices.is_empty() {
                    continue;
                }
                let bytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
                let iv = w.view(&bytes, Some(ELEMENT_ARRAY_BUFFER));
                let accessor = w.accessor(iv, UNSIGNED_INT, indices.len(), "SCALAR", "");
                primitives.push(format!(
                    "{{\"attributes\":{attributes},\"indices\":{accessor},\"material\":{},\"mode\":4}}",
                    material.unwrap_or(0)
                ));
            }
            meshes.push(format!(
                "{{\"name\":{},\"primitives\":[{}]}}",
                quote(part.name),
                primitives.join(",")
            ));
            let _ = write!(node, ",\"mesh\":{}", meshes.len() - 1);
        }
        node.push('}');
        nodes.push(node);
    }
    let mut json = String::new();
    let _ = write!(
        json,
        "{{\"asset\":{{\"version\":\"2.0\",\"generator\":\"meshbox {}\"}}",
        env!("CARGO_PKG_VERSION")
    );
    let node_list: Vec<String> = (0..nodes.len()).map(|i| i.to_string()).collect();
    let _ = write!(
        json,
        ",\"scene\":0,\"scenes\":[{{\"nodes\":[{}]}}],\"nodes\":[{}]",
        node_list.join(","),
        nodes.join(",")
    );
    if !meshes.is_empty() {
        let png = png.ok_or_else(|| {
            Error::new(
                ErrorCode::EncodeError,
                "A mesh needs the palette texture, and the document has none",
            )
        })?;
        let image_view = w.view(png, None);
        let _ = write!(json, ",\"meshes\":[{}]", meshes.join(","));
        let mut materials = Vec::new();
        let pbr = "\"pbrMetallicRoughness\":{\"baseColorTexture\":{\"index\":0},\"metallicFactor\":0,\"roughnessFactor\":1}";
        if any_opaque {
            materials.push(format!("{{\"name\":\"opaque\",{pbr}}}"));
        }
        if any_clear {
            materials.push(format!(
                "{{\"name\":\"translucent\",{pbr},\"alphaMode\":\"BLEND\"}}"
            ));
        }
        let _ = write!(
            json,
            ",\"materials\":[{}],\"textures\":[{{\"sampler\":0,\"source\":0}}],\"images\":[{{\"bufferView\":{image_view},\"mimeType\":\"image/png\"}}],\"samplers\":[{{\"magFilter\":{NEAREST},\"minFilter\":{NEAREST},\"wrapS\":{CLAMP_TO_EDGE},\"wrapT\":{CLAMP_TO_EDGE}}}]",
            materials.join(",")
        );
        let _ = write!(
            json,
            ",\"accessors\":[{}],\"bufferViews\":[{}],\"buffers\":[{{\"byteLength\":{}}}]",
            w.accessors.join(","),
            w.views.join(","),
            w.bin.len()
        );
    }
    json.push('}');

    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    w.align();
    let has_bin = !w.bin.is_empty();
    let total = 12 + 8 + json.len() + if has_bin { 8 + w.bin.len() } else { 0 };
    let total = u32::try_from(total)
        .map_err(|_| Error::new(ErrorCode::EncodeError, "The glb is larger than 4 GiB"))?;
    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    if has_bin {
        out.extend_from_slice(&(w.bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&w.bin);
    }
    Ok(out)
}

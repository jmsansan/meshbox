mod common;

use common::json::{self, Json};
use meshbox::Document;

const TEXT: &str = "\
palette p
  .  transparent
  a  #c0392b
  b  #f4f4f4
  g  #80c0ff80

voxels block 3x2x2 palette=p
  level 0
    aab
    abb
  level 1
    g..
    ...

voxels solid 1x1x1 palette=p
  level 0
    a

voxels empty 2x2x1 palette=p
";

/// A glb's JSON and binary chunk, with the container checked.
fn read(glb: &[u8]) -> (Json, Vec<u8>) {
    assert_eq!(&glb[0..4], b"glTF");
    assert_eq!(u32::from_le_bytes(glb[4..8].try_into().unwrap()), 2);
    assert_eq!(
        u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize,
        glb.len()
    );
    let json_len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    assert_eq!(&glb[16..20], b"JSON");
    assert_eq!(json_len % 4, 0, "JSON chunk is padded to 4 bytes");
    let text = std::str::from_utf8(&glb[20..20 + json_len]).unwrap();
    let doc = json::parse(text.trim_end_matches(' '));
    let rest = &glb[20 + json_len..];
    if rest.is_empty() {
        return (doc, Vec::new());
    }
    let bin_len = u32::from_le_bytes(rest[0..4].try_into().unwrap()) as usize;
    assert_eq!(&rest[4..8], b"BIN\0");
    assert_eq!(bin_len % 4, 0, "BIN chunk is padded to 4 bytes");
    assert_eq!(rest.len(), 8 + bin_len, "nothing after the BIN chunk");
    (doc, rest[8..].to_vec())
}

fn floats(bin: &[u8], doc: &Json, accessor: usize) -> Vec<f32> {
    let a = doc.get("accessors").at(accessor);
    let view = doc.get("bufferViews").at(a.get("bufferView").usize());
    let offset = view.get("byteOffset").usize();
    let len = view.get("byteLength").usize();
    bin[offset..offset + len]
        .chunks(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

#[test]
fn writes_a_valid_glb() {
    let document = Document::parse(TEXT).unwrap();
    let glb = document.glb("block").unwrap();
    let (doc, bin) = read(&glb);
    assert_eq!(doc.get("asset").get("version").str(), "2.0");
    assert_eq!(
        doc.get("asset").get("generator").str(),
        concat!("meshbox ", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(doc.get("scene").usize(), 0);
    assert_eq!(doc.get("scenes").at(0).get("nodes").len(), 1);
    let node = doc.get("nodes").at(0);
    assert_eq!(node.get("name").str(), "block");
    assert!(
        node.get("translation").is_null(),
        "the node is at the model's origin"
    );
    let mesh_json = doc.get("meshes").at(node.get("mesh").usize());
    let primitives = mesh_json.get("primitives");
    // Opaque and translucent triangles in two primitives, sharing the vertices.
    assert_eq!(primitives.len(), 2);
    let mesh = document.mesh("block").unwrap();
    let (opaque, clear) = (primitives.at(0), primitives.at(1));
    assert_eq!(opaque.get("attributes"), clear.get("attributes"));
    let attributes = opaque.get("attributes");
    let accessors = doc.get("accessors");
    let position = accessors.at(attributes.get("POSITION").usize());
    assert_eq!(position.get("count").usize(), mesh.positions.len());
    assert_eq!(position.get("componentType").usize(), 5126);
    assert_eq!(position.get("type").str(), "VEC3");
    let bounds = mesh.bounds().unwrap();
    for i in 0..3 {
        assert_eq!(position.get("min").at(i).num() as f32, bounds.min[i]);
        assert_eq!(position.get("max").at(i).num() as f32, bounds.max[i]);
    }
    assert_eq!(
        accessors
            .at(attributes.get("TEXCOORD_0").usize())
            .get("type")
            .str(),
        "VEC2"
    );
    let indices = accessors.at(opaque.get("indices").usize());
    assert_eq!(indices.get("count").usize(), mesh.opaque_indices().len());
    assert_eq!(indices.get("componentType").usize(), 5125);
    assert_eq!(
        accessors
            .at(clear.get("indices").usize())
            .get("count")
            .usize(),
        mesh.translucent_indices().len()
    );
    // The vertex data in the binary chunk is the mesh's.
    let positions: Vec<f32> = mesh.positions.iter().flatten().copied().collect();
    assert_eq!(
        floats(&bin, &doc, attributes.get("POSITION").usize()),
        positions
    );
    let uvs: Vec<f32> = mesh.uvs.iter().flatten().copied().collect();
    assert_eq!(
        floats(&bin, &doc, attributes.get("TEXCOORD_0").usize()),
        uvs
    );
    // Every view is in the buffer and 4-byte aligned.
    let buffer = doc.get("buffers").at(0).get("byteLength").usize();
    assert!(buffer <= bin.len() && bin.len() - buffer < 4);
    for i in 0..doc.get("bufferViews").len() {
        let view = doc.get("bufferViews").at(i);
        let offset = view.get("byteOffset").usize();
        assert_eq!(offset % 4, 0);
        assert!(offset + view.get("byteLength").usize() <= buffer);
    }
    // Materials: one opaque, one blended, both sampling the palette texture.
    let materials = doc.get("materials");
    let opaque_material = materials.at(opaque.get("material").usize());
    let clear_material = materials.at(clear.get("material").usize());
    assert!(opaque_material.get("alphaMode").is_null());
    assert_eq!(clear_material.get("alphaMode").str(), "BLEND");
    let pbr = opaque_material.get("pbrMetallicRoughness");
    assert_eq!(pbr.get("metallicFactor").num(), 0.0);
    assert_eq!(pbr.get("roughnessFactor").num(), 1.0);
    assert_eq!(pbr.get("baseColorTexture").get("index").usize(), 0);
    let sampler = doc.get("samplers").at(0);
    assert_eq!(sampler.get("magFilter").usize(), 9728);
    assert_eq!(sampler.get("minFilter").usize(), 9728);
    assert_eq!(sampler.get("wrapS").usize(), 33071);
    assert_eq!(sampler.get("wrapT").usize(), 33071);
    // The image is the palette texture as PNG.
    let image = doc.get("images").at(0);
    assert_eq!(image.get("mimeType").str(), "image/png");
    let view = doc.get("bufferViews").at(image.get("bufferView").usize());
    let offset = view.get("byteOffset").usize();
    let png = &bin[offset..offset + view.get("byteLength").usize()];
    let texture = document.palette_texture().unwrap().unwrap();
    assert_eq!(common::decode_png(png).pixels, texture.image().pixels());
}

#[test]
fn opaque_only_models_have_one_material() {
    let document = Document::parse(TEXT).unwrap();
    let (doc, _) = read(&document.glb("solid").unwrap());
    assert_eq!(doc.get("materials").len(), 1);
    assert_eq!(doc.get("meshes").at(0).get("primitives").len(), 1);
}

#[test]
fn an_empty_model_is_a_node_without_a_mesh() {
    let document = Document::parse(TEXT).unwrap();
    let (doc, bin) = read(&document.glb("empty").unwrap());
    assert_eq!(doc.get("nodes").at(0).get("name").str(), "empty");
    assert!(doc.get("nodes").at(0).get("mesh").is_null());
    assert!(doc.get("meshes").is_null() && doc.get("buffers").is_null());
    assert!(bin.is_empty());
}

#[test]
fn the_same_document_gives_the_same_bytes() {
    let a = Document::parse(TEXT)
        .unwrap()
        .export()
        .unwrap()
        .glb_files()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let b = Document::parse(TEXT)
        .unwrap()
        .export()
        .unwrap()
        .glb_files()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(a, b);
    let names: Vec<&str> = a.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        ["model-block.glb", "model-solid.glb", "model-empty.glb"]
    );
    assert_eq!(a[0].1, Document::parse(TEXT).unwrap().glb("block").unwrap());
}

#[test]
fn an_export_shares_one_palette_png() {
    let document = Document::parse(TEXT).unwrap();
    let export = document.export().unwrap();
    let texture = document.palette_texture().unwrap().unwrap();
    assert_eq!(
        export.palette_png().unwrap(),
        meshbox::png::encode(texture.image())
    );
    let names: Vec<&str> = export.meshes().map(|(n, _)| n).collect();
    assert_eq!(names, ["block", "solid", "empty"]);
    assert_eq!(
        export.meshes().next().unwrap().1,
        &document.mesh("block").unwrap()
    );
}

#[test]
fn origins_and_units_keep_positions_finite() {
    for header in [
        "origin=100000000000000000000,0,0",
        "origin=0,65537,0",
        "unit=100000000000000000000",
        "unit=2000",
        "unit=0.00000001",
    ] {
        let text = format!("palette p\n  a #ffffff\nvoxels m 1x1x1 palette=p {header}\n");
        let errors = meshbox::check(&text);
        assert_eq!(errors.len(), 1, "{header}");
        assert_eq!(
            errors[0].code,
            meshbox::ErrorCode::InvalidArgument,
            "{header}"
        );
    }
    assert!(
        meshbox::check(
            "palette p\n  a #ffffff\nvoxels m 1x1x1 palette=p origin=65536,0,-65536 unit=1024\n"
        )
        .is_empty()
    );
}

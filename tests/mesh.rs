mod common;

use std::collections::HashMap;

use meshbox::{Document, ErrorCode, Limits, Mesh};

/// A model of `size` (`WxDxH`) whose voxels are `key(x, y, z)`, or `.`.
fn model(size: [u32; 3], palette: &str, key: impl Fn(u32, u32, u32) -> char) -> String {
    let [w, d, h] = size;
    let mut text = format!("{palette}\nvoxels m {w}x{d}x{h} palette=p origin=0,0,0 unit=1\n");
    for y in 0..h {
        text.push_str(&format!("  level {y}\n"));
        for z in 0..d {
            text.push_str("    ");
            for x in 0..w {
                text.push(key(x, y, z));
            }
            text.push('\n');
        }
    }
    text
}

const OPAQUE: &str = "palette p\n  a #ff0000\n  b #00ff00\n  g #8080ff80";

fn mesh_of(text: &str) -> Mesh {
    Document::parse(text).unwrap().mesh("m").unwrap()
}

#[test]
fn a_solid_cube_is_twelve_triangles() {
    let mesh = mesh_of(&model([8, 8, 8], OPAQUE, |_, _, _| 'a'));
    let stats = mesh.stats();
    assert_eq!(stats.triangles, 12);
    assert_eq!(stats.vertices, 24);
    assert_eq!(stats.naive_triangles, 8 * 8 * 6 * 2);
    let b = mesh.bounds().unwrap();
    assert_eq!((b.min, b.max), ([0.0; 3], [8.0; 3]));
}

#[test]
fn a_checkerboard_keeps_every_face() {
    let mesh = mesh_of(&model([4, 4, 1], OPAQUE, |x, _, z| {
        if (x + z) % 2 == 0 { 'a' } else { 'b' }
    }));
    let stats = mesh.stats();
    // The top and bottom keep 16 faces each; each side merges nothing either.
    assert_eq!(stats.triangles, stats.naive_triangles);
    assert_eq!(stats.triangles, (16 + 16 + 4 * 4) * 2);
}

#[test]
fn a_hollow_box_has_inward_faces() {
    let mesh = mesh_of(&model([3, 3, 3], OPAQUE, |x, y, z| {
        if (x, y, z) == (1, 1, 1) { '.' } else { 'a' }
    }));
    assert_eq!(mesh.stats().triangles, 12 + 12);
    // The cavity's faces point into it: the face on the plane x = 1 faces +X.
    let inward = (0..mesh.positions.len()).filter(|&i| {
        let p = mesh.positions[i];
        p[0] == 1.0 && (1.0..=2.0).contains(&p[1]) && (1.0..=2.0).contains(&p[2])
    });
    for i in inward {
        if mesh.normals[i][0] != 0.0 {
            assert_eq!(mesh.normals[i], [1.0, 0.0, 0.0]);
        }
    }
    assert!(mesh.normals.contains(&[-1.0, 0.0, 0.0]));
}

#[test]
fn quads_wind_counter_clockwise_toward_their_normal() {
    let mesh = mesh_of(&model(
        [2, 3, 2],
        OPAQUE,
        |x, y, _| if x + y < 3 { 'a' } else { 'b' },
    ));
    for t in mesh.indices.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[t[i] as usize]);
        let (u, v) = (sub(b, a), sub(c, a));
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let n = mesh.normals[t[0] as usize];
        let dot = cross[0] * n[0] + cross[1] * n[1] + cross[2] * n[2];
        assert!(dot > 0.0, "{t:?}");
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Every edge of the surface, cut into unit lengths, is used once in each direction:
/// the mesh is closed even where a large rectangle meets two small ones. Where voxels
/// touch along an edge only, four faces share it, so a shape that is not a manifold
/// is checked for closure alone: each edge is used as often in each direction.
fn assert_watertight(mesh: &Mesh, manifold: bool) {
    let mut edges: HashMap<([i64; 3], [i64; 3]), i32> = HashMap::new();
    let point = |i: u32| mesh.positions[i as usize].map(|v| v as i64);
    for t in mesh.indices.chunks(3) {
        for k in 0..3 {
            let (a, b) = (point(t[k]), point(t[(k + 1) % 3]));
            let differ = (0..3).filter(|&i| a[i] != b[i]).count();
            if differ == 1 {
                let axis = (0..3).find(|&i| a[i] != b[i]).unwrap();
                let step = (b[axis] - a[axis]).signum();
                let mut p = a;
                while p != b {
                    let mut q = p;
                    q[axis] += step;
                    *edges.entry((p, q)).or_default() += 1;
                    p = q;
                }
            } else {
                // A quad's diagonal.
                *edges.entry((a, b)).or_default() += 1;
            }
        }
    }
    for (&(a, b), &count) in &edges {
        let reverse = edges.get(&(b, a)).copied().unwrap_or(0);
        if manifold {
            assert_eq!(count, 1, "edge {a:?} -> {b:?} used {count} times");
        }
        assert_eq!(
            reverse, count,
            "edge {a:?} -> {b:?}: {count} uses, {reverse} reversed"
        );
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn random_model(side: u32, seed: u64, keys: &[char]) -> String {
    let mut rng = Rng(seed);
    let mut cells = Vec::new();
    for _ in 0..side * side * side {
        cells.push(keys[(rng.next() % keys.len() as u64) as usize]);
    }
    model([side, side, side], OPAQUE, |x, y, z| {
        cells[((y * side + z) * side + x) as usize]
    })
}

#[test]
fn closed_opaque_models_are_watertight() {
    assert_watertight(&mesh_of(&model([8, 8, 8], OPAQUE, |_, _, _| 'a')), true);
    // A staircase in two keys, where large rectangles meet small ones.
    let stairs = model([5, 4, 3], OPAQUE, |x, y, z| match x <= y + z {
        true if (x + z) % 3 == 0 => 'b',
        true => 'a',
        false => '.',
    });
    assert_watertight(&mesh_of(&stairs), true);
    // A hollow box, inside and out.
    let hollow = model([4, 4, 4], OPAQUE, |x, y, z| {
        let inside = |v: u32| (1..3).contains(&v);
        if inside(x) && inside(y) && inside(z) {
            '.'
        } else {
            'a'
        }
    });
    assert_watertight(&mesh_of(&hollow), true);
    for seed in 1..20 {
        assert_watertight(
            &mesh_of(&random_model(6, seed, &['a', 'b', '.', '.'])),
            false,
        );
    }
}

#[test]
fn translucent_voxels_do_not_hide_what_is_behind_them() {
    // Glass in front of a red voxel: the red face behind the glass stays.
    let mesh = mesh_of(&model(
        [1, 2, 1],
        OPAQUE,
        |_, _, z| if z == 0 { 'a' } else { 'g' },
    ));
    let opaque = mesh.opaque_indices().len() / 3;
    let clear = mesh.translucent_indices().len() / 3;
    // Red: six faces. Glass: five, as its back touches the opaque voxel.
    assert_eq!((opaque, clear), (12, 10));
    // Two glass voxels side by side are one block: no face between them.
    let mesh = mesh_of(&model([2, 1, 1], OPAQUE, |_, _, _| 'g'));
    assert_eq!(mesh.translucent_indices().len() / 3, 12);
    assert_eq!(mesh.translucent_start, 0);
}

#[test]
fn uvs_point_at_the_keys_texel() {
    let doc = Document::parse(&model(
        [2, 1, 1],
        OPAQUE,
        |x, _, _| if x == 0 { 'a' } else { 'b' },
    ))
    .unwrap();
    let texture = doc.palette_texture().unwrap().unwrap();
    let mesh = doc.mesh("m").unwrap();
    let a = texture.uv("p", 'a').unwrap();
    let b = texture.uv("p", 'b').unwrap();
    for (p, uv) in mesh.positions.iter().zip(&mesh.uvs) {
        assert!(*uv == a || *uv == b);
        if p[0] == 0.0 {
            assert_eq!(*uv, a);
        }
        if p[0] == 2.0 {
            assert_eq!(*uv, b);
        }
    }
}

#[test]
fn positions_are_relative_to_the_origin_in_meters() {
    let text = "palette p\n  a #ffffff\nvoxels m 2x2x1 palette=p\n  level 0\n    aa\n    aa\n";
    let b = mesh_of(text).bounds().unwrap();
    // The default origin is the bottom face's center; a voxel is 1/16 m.
    assert_eq!(b.min, [-0.0625, 0.0, -0.0625]);
    assert_eq!(b.max, [0.0625, 0.0625, 0.0625]);
}

#[test]
fn the_same_model_gives_the_same_bytes() {
    // Golden hashes: the same on every platform. Changing the mesher's output
    // changes them, deliberately.
    let mesh = mesh_of(&random_model(7, 42, &['a', 'b', 'g', '.']));
    assert_eq!(
        mesh.to_bytes(),
        mesh_of(&random_model(7, 42, &['a', 'b', 'g', '.'])).to_bytes()
    );
    let golden = [
        common::fnv(&mesh.to_bytes()),
        common::fnv(
            &mesh_of(&model([5, 4, 3], OPAQUE, |x, y, z| {
                if x <= y + z { 'a' } else { 'g' }
            }))
            .to_bytes(),
        ),
    ];
    assert_eq!(
        golden,
        [0x86cc8a6baaab693c, 0x6b6b76ef28da40cf],
        "{golden:x?}"
    );
}

#[test]
fn limits_bound_the_mesh() {
    let text = random_model(16, 3, &['a', 'b']);
    let doc = Document::parse(&text).unwrap();
    let limits = Limits {
        max_triangles: 100,
        ..Limits::default()
    };
    assert_eq!(
        doc.mesh_with("m", &limits).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
    let limits = Limits {
        max_vertices: 100,
        ..Limits::default()
    };
    assert_eq!(
        doc.mesh_with("m", &limits).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(doc.mesh("other").unwrap_err().code, ErrorCode::UnknownName);
}

#[test]
fn meshing_64_cubed_is_fast() {
    let text = random_model(64, 7, &['a', 'b', '.', 'g']);
    let doc = Document::parse(&text).unwrap();
    let start = std::time::Instant::now();
    let mesh = doc.mesh("m").unwrap();
    let elapsed = start.elapsed();
    eprintln!("meshed 64x64x64 in {elapsed:?}");
    assert!(mesh.stats().triangles > 0);
    if !cfg!(debug_assertions) {
        assert!(elapsed.as_millis() < 100, "{elapsed:?}");
    }
}

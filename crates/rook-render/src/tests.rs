use rustc_hash::FxHashMap;
use std::sync::Arc;

use glam::Mat3;
use rook_types::{NormalDetail, SurfacePattern};

use super::*;

mod glass;
mod gui;
mod images;
mod lighting;
mod maps;
mod parts;

fn part(shape: PartShape, x: f32, color: [f32; 3], transparency: f32) -> PartInstance {
    PartInstance {
        color,
        transparency,
        ..PartInstance::plain(
            DrawShape::Part(shape),
            Vec3::new(x, 0.0, 0.0),
            Mat3::IDENTITY,
            Vec3::splat(2.0),
        )
    }
}

/// A 2 × 2 image: red, green / blue, white.
fn image() -> Arc<Image> {
    let pixels = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255; 4],
    ];
    Arc::new(Image {
        width: 2,
        height: 2,
        format: rook_content::decode::image::ImageFormat::Png,
        encoding: rook_content::decode::image::Encoding::Rgba8,
        mips: rook_content::decode::image::mips(pixels.concat(), 2, 2),
        translucent: false,
        gui_size: (2, 2),
    })
}

/// A small mesh file: a square of two triangles facing +Z, `side` wide.
fn square(side: f32) -> Arc<Mesh> {
    let h = side / 2.0;
    Arc::new(Mesh {
        positions: vec![[-h, -h, 0.0], [h, -h, 0.0], [h, h, 0.0], [-h, h, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Mesh::default()
    })
}

/// A scene drawing with everything that binds images or meshes: mesh
/// files, a mesh texture, material maps, a decal, transparent parts,
/// Glass, a ForceField and a skybox.
fn busy_scene() -> (Scene, Vec<(ImageHash, Arc<Image>)>) {
    let (a, b) = (ImageHash([1; 32]), ImageHash([2; 32]));
    let (m1, m2) = (MeshHash([3; 32]), MeshHash([4; 32]));
    let mut parts = vec![
        part(PartShape::Block, -6.0, [1.0, 0.0, 0.0], 0.0),
        part(PartShape::Ball, -3.0, [0.0, 1.0, 0.0], 0.0),
        part(PartShape::Block, 0.0, [0.0, 0.0, 1.0], 0.5),
        part(PartShape::Cylinder, 3.0, [1.0, 1.0, 0.0], 0.0),
        part(PartShape::Wedge, 6.0, [0.0, 1.0, 1.0], 0.0),
    ];
    parts[1].texture = Some(a);
    parts[3].maps = MaterialMaps {
        color: Some(b),
        normal: Some(a),
        ..MaterialMaps::NONE
    };
    let mut glass = part(PartShape::Block, -1.5, [0.5, 0.8, 1.0], 0.3);
    glass.material = Material::Glass;
    let mut field = part(PartShape::Ball, 1.5, [1.0, 0.2, 0.2], 0.0);
    field.material = Material::ForceField;
    parts.extend([glass, field]);
    for (i, mesh) in [m1, m2, m1].into_iter().enumerate() {
        let mut p = part(PartShape::Block, -4.0 + i as f32 * 4.0, [0.9; 3], 0.0);
        p.shape = DrawShape::File(mesh);
        p.position.y = 3.0;
        p.size = Vec3::ONE;
        p.texture = (i == 1).then_some(b);
        parts.push(p);
    }
    parts.push(PartInstance {
        layer: Layer::Image {
            image: b,
            placement: ImagePlacement::Face(NormalId::Back),
            z: 1,
            on_transparent: false,
        },
        ..part(PartShape::Block, -6.0, [1.0; 3], 0.0)
    });
    let scene = Scene {
        parts,
        meshes: FxHashMap::from_iter([(m1, square(2.0)), (m2, square(1.5))]),
        lighting: Lighting {
            skybox: Some([a, b, a, b, a, b]),
            ..Lighting::default()
        },
        ..Scene::default()
    };
    (scene, vec![(a, image()), (b, image())])
}

/// A red square 4 studs wide facing +Z, double-sided or not, in a scene of
/// a sun at `sun` alone.
fn square_scene(double_sided: bool, sun: Vec3) -> Scene {
    let mesh = MeshHash([9; 32]);
    Scene {
        parts: vec![PartInstance {
            color: [0.8, 0.1, 0.1],
            double_sided,
            ..PartInstance::plain(DrawShape::File(mesh), Vec3::ZERO, Mat3::IDENTITY, Vec3::ONE)
        }],
        meshes: FxHashMap::from_iter([(mesh, square(4.0))]),
        lighting: Lighting {
            sun_direction: sun.normalize(),
            ..Lighting::default()
        },
        ..Scene::default()
    }
}

/// The pixel at the middle of a 32 x 32 render.
fn middle(r: &mut Renderer, scene: &Scene, camera: &Camera) -> [u8; 3] {
    r.set_scene(scene);
    r.render_to_image(32, 32, camera).unwrap();
    let pixels = r.render_to_image(32, 32, camera).unwrap();
    let i = (16 * 32 + 16) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

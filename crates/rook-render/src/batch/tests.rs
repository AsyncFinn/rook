use glam::{Mat3, Vec3};

use super::*;
use crate::Camera;
use cull::Frustum;

/// An instance placed where a value isn't a number (here a mesh whose
/// bounds came out NaN) hides no other batch: batches are culled in
/// blocks under one joined box, and NaN joined in would drop the boxes
/// before it.
#[test]
fn an_instance_that_isnt_a_number_hides_no_other_batch() {
    let ball = DrawShape::Part(PartShape::Ball);
    let block = DrawShape::Part(PartShape::Block);
    let part = |shape| PartInstance::plain(shape, Vec3::NEG_Z * 10.0, Mat3::IDENTITY, Vec3::ONE);
    let key = |shape| BatchKey {
        shape,
        images: Default::default(),
        double_sided: false,
    };
    let mut batches = Batches::default();
    let unit = (Vec3::splat(-0.5), Vec3::splat(0.5));
    batches.occupy(0, key(block), 0, Instance::new(&part(block), unit, [0; 3]));
    let nan = Instance::new(&part(ball), (Vec3::NAN, Vec3::NAN), [0; 3]);
    batches.occupy(0, key(ball), 1, nan);
    for degrees in -40..=40 {
        let turn = Mat3::from_rotation_y((degrees as f32).to_radians());
        let camera = Camera::look_at(Vec3::ZERO, turn * Vec3::NEG_Z);
        let view = Frustum::of(camera.view_projection(16.0 / 9.0));
        assert!(batches.seen(&view, 0.0)[0], "turned {degrees} degrees");
    }
}

/// A part that moves away and back leaves its batch's box tight at the
/// next upload, however many parts the batch holds: culling isn't left
/// drawing a batch for where one part once was.
#[test]
fn a_part_moved_back_leaves_its_batch_tight() {
    let block = DrawShape::Part(PartShape::Block);
    let key = BatchKey {
        shape: block,
        images: Default::default(),
        double_sided: false,
    };
    let unit = (Vec3::splat(-0.5), Vec3::splat(0.5));
    let at = |x: f32| {
        let part = PartInstance::plain(block, Vec3::new(x, 0.0, 0.0), Mat3::IDENTITY, Vec3::ONE);
        Instance::new(&part, unit, [0; 3])
    };
    let mut batches = Batches::default();
    let (batch, slot) = batches.occupy(0, key, 0, at(0.0));
    for i in 1..100 {
        batches.occupy(0, key, i, at(0.0));
    }
    batches.refit();
    // Looking at x = 1000 from beside it: nothing of the batch is there.
    let camera = Camera::look_at(Vec3::new(990.0, 0.0, 0.0), Vec3::new(1000.0, 0.0, 0.0));
    let view = Frustum::of(camera.view_projection(1.0));
    batches.set(batch, slot, at(1000.0));
    batches.refit();
    assert!(batches.seen(&view, 0.0)[0], "there while it is");
    batches.set(batch, slot, at(0.0));
    batches.refit();
    assert!(!batches.seen(&view, 0.0)[0], "gone once it moved back");
}

//! Images by `ScaleType`, checked against Studio 0.743 drawing a 64 × 48
//! image (docs/behavior/gui/images.md).

use rook_types::{Content, Paint};

use super::*;

const URL: &str = "rbxasset://textures/test.png";

/// An ImageLabel `w` × `h` in the box showing the 64 × 48 image once it
/// has loaded, with `props` set.
fn label(w: i32, h: i32, props: &[(&str, Value)]) -> (Dom, Gui, InstanceId) {
    let (mut dom, frame) = boxed();
    let mut all = vec![
        ("Size", udim2(0.0, w, 0.0, h)),
        ("BackgroundTransparency", Value::Float32(1.0)),
        ("ImageContent", Value::Content(Content::from_uri(URL))),
    ];
    all.extend(props.iter().cloned());
    let id = child(&mut dom, "ImageLabel", frame, &all);
    let mut gui = laid_out(&mut dom);
    gui.draw(&dom);
    assert_eq!(gui.take_image_requests(), [URL]);
    assert_eq!(dom.bool_property(id, "IsLoaded"), Some(false));
    let image = GuiImage {
        hash: [1; 32],
        width: 64,
        height: 48,
    };
    gui.image_arrived(URL, Some(image));
    gui.sync(&mut dom);
    assert_eq!(dom.bool_property(id, "IsLoaded"), Some(true));
    (dom, gui, id)
}

/// Each piece of image drawn: its top left from the label's, its size,
/// and the part of the image it shows.
fn pieces(dom: &Dom, gui: &mut Gui, id: InstanceId) -> Vec<([f32; 2], [f32; 2], [f32; 4])> {
    // Drawn in the window, below the top bar.
    let origin = vec2_of(dom, id, "AbsolutePosition") + Vec2::new(0.0, GUI_INSET);
    let round = |v: Vec2| (v * 1000.0).round().to_array().map(|x| x / 1000.0);
    gui.draw(dom)
        .iter()
        .filter_map(|q| match q.paint {
            Paint::Image(p) => Some((
                round(q.center - q.size * 0.5 - origin),
                round(q.size),
                p.uv.map(|u| (u * 1000.0).round() / 1000.0),
            )),
            _ => None,
        })
        .collect()
}

fn scale_type(
    item: &str,
    w: i32,
    h: i32,
    props: &[(&str, Value)],
) -> Vec<([f32; 2], [f32; 2], [f32; 4])> {
    let (mut dom, mut gui, id) = label(w, h, props);
    set_enum(&mut dom, id, "ScaleType", item);
    gui.sync(&mut dom);
    pieces(&dom, &mut gui, id)
}

#[test]
fn stretch_fit_and_crop() {
    let whole = [0.0, 0.0, 1.0, 1.0];
    assert_eq!(
        scale_type("Stretch", 180, 120, &[]),
        [([0.0, 0.0], [180.0, 120.0], whole)]
    );
    assert_eq!(
        scale_type("Fit", 180, 120, &[]),
        [([10.0, 0.0], [160.0, 120.0], whole)]
    );
    // 135 high at 180 wide: 7.5 of it cut off top and bottom.
    assert_eq!(
        scale_type("Crop", 180, 120, &[]),
        [([0.0, 0.0], [180.0, 120.0], [0.0, 0.056, 1.0, 0.944])]
    );
    // A 64 × 16 strip fitted in a square.
    let strip = [("ImageRectSize", v2(64.0, 16.0))];
    assert_eq!(
        scale_type("Fit", 120, 120, &strip),
        [([0.0, 45.0], [120.0, 30.0], [0.0, 0.0, 1.0, 0.333])]
    );
}

#[test]
fn image_rects_mirror_and_stop_at_the_edges() {
    let rect = |offset: [f32; 2], size: [f32; 2]| {
        let props = [
            ("ImageRectOffset", v2(offset[0], offset[1])),
            ("ImageRectSize", v2(size[0], size[1])),
        ];
        scale_type("Stretch", 128, 96, &props)[0].2
    };
    assert_eq!(rect([64.0, 0.0], [-64.0, 48.0]), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(rect([32.0, 24.0], [64.0, 48.0]), [0.5, 0.5, 1.0, 1.0]);
    // One axis of 0 is a line of texels, not the whole image.
    assert_eq!(rect([0.0, 0.0], [0.0, 16.0]), [0.0, 0.0, 0.0, 0.333]);
}

#[test]
fn tiles_repeat_from_the_top_left() {
    let (mut dom, mut gui, id) = label(180, 120, &[("TileSize", udim2(0.0, 48, 0.0, 36))]);
    set_enum(&mut dom, id, "ScaleType", "Tile");
    gui.sync(&mut dom);
    let quads = gui.draw(&dom);
    let Some(Paint::Image(p)) = quads.last().map(|q| q.paint.clone()) else {
        panic!("no image");
    };
    assert!(p.tiled);
    assert_eq!(
        p.uv.map(|u| (u * 100.0).round() / 100.0),
        [0.0, 0.0, 3.75, 3.33]
    );
    // A TileSize of 0 draws it once; an ImageRect stretches it.
    let once = [("TileSize", udim2(0.0, 0, 0.0, 0))];
    assert_eq!(
        scale_type("Tile", 120, 90, &once)[0].2,
        [0.0, 0.0, 1.0, 1.0]
    );
    let rect = [("ImageRectSize", v2(32.0, 16.0))];
    assert_eq!(scale_type("Tile", 120, 90, &rect)[0].1, [120.0, 90.0]);
}

/// The 64 × 48 image's center is (16, 16) to (48, 32).
fn sliced(w: i32, h: i32, props: &[(&str, Value)]) -> Vec<([f32; 2], [f32; 2], [f32; 4])> {
    let mut all = vec![("SliceCenter", center([16.0, 16.0], [48.0, 32.0]))];
    all.extend(props.iter().cloned());
    scale_type("Slice", w, h, &all)
}

/// A `SliceCenter` from `min` to `max`.
fn center(min: [f32; 2], max: [f32; 2]) -> Value {
    let corner = |c: [f32; 2]| Vector2::new(c[0], c[1]).into();
    Value::Rect(rook_types::Rect::new(corner(min), corner(max)))
}

#[test]
fn slices_keep_their_corners() {
    let nine = sliced(180, 120, &[]);
    assert_eq!(nine.len(), 9);
    assert_eq!(nine[0], ([0.0, 0.0], [16.0, 16.0], [0.0, 0.0, 0.25, 0.333]));
    assert_eq!(
        nine[4],
        ([16.0, 16.0], [148.0, 88.0], [0.25, 0.333, 0.75, 0.667])
    );
    assert_eq!(
        nine[8],
        ([164.0, 104.0], [16.0, 16.0], [0.75, 0.667, 1.0, 1.0])
    );
    let doubled = sliced(180, 120, &[("SliceScale", Value::Float32(2.0))]);
    assert_eq!(doubled[0].1, [32.0, 32.0]);
    // Too small for its corners: they shrink together, 30 / 32.
    let small = sliced(40, 30, &[]);
    assert_eq!(small.len(), 6);
    assert_eq!(small[0].1, [15.0, 15.0]);
    assert_eq!(
        small[1],
        ([15.0, 0.0], [10.0, 15.0], [0.25, 0.0, 0.75, 0.333])
    );
}

#[test]
fn odd_slice_centers() {
    // (0, 0, 0, 0) is taken as it is: the whole image is the corner.
    let (mut dom, mut gui, id) = label(180, 120, &[]);
    set_enum(&mut dom, id, "ScaleType", "Slice");
    gui.sync(&mut dom);
    let zero = pieces(&dom, &mut gui, id);
    assert_eq!(zero.len(), 4);
    assert_eq!(zero[0], ([0.0, 0.0], [116.0, 72.0], [0.0; 4]));
    assert_eq!(zero[3], ([116.0, 72.0], [64.0, 48.0], [0.0, 0.0, 1.0, 1.0]));
    // Corners the wrong way round are swapped.
    let swapped = center([48.0, 32.0], [16.0, 16.0]);
    assert_eq!(
        sliced(180, 120, &[("SliceCenter", swapped)]),
        sliced(180, 120, &[])
    );
    // A SliceScale of 0 fills it with the bottom right corner.
    assert_eq!(
        sliced(180, 120, &[("SliceScale", Value::Float32(0.0))]),
        [([0.0, 0.0], [180.0, 120.0], [0.75, 0.667, 1.0, 1.0])]
    );
    // Within an ImageRect, the center counts from its corner.
    let within = sliced(
        180,
        120,
        &[
            ("ImageRectOffset", v2(16.0, 16.0)),
            ("ImageRectSize", v2(48.0, 32.0)),
            ("SliceCenter", center([8.0, 8.0], [40.0, 24.0])),
        ],
    );
    assert_eq!(within[0].2, [0.25, 0.333, 0.375, 0.5]);
}

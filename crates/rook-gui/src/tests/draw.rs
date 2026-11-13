use rook_types::{Color3, GuiQuad, Paint};

use super::*;

/// A borderless 200 × 200 Frame at (600, 300) with this ZIndex, colored
/// by its index so draws can be told apart.
fn square(dom: &mut Dom, parent: InstanceId, z: i32, tag: f32) -> InstanceId {
    child(
        dom,
        "Frame",
        parent,
        &[
            ("Size", udim2(0.0, 200, 0.0, 200)),
            ("Position", udim2(0.0, 600, 0.0, 300)),
            ("ZIndex", Value::Int32(z)),
            ("BorderSizePixel", Value::Int32(0)),
            (
                "BackgroundColor3",
                Value::Color3(Color3::new(tag, 0.0, 0.0)),
            ),
        ],
    )
}

fn order(dom: &mut Dom) -> Vec<f32> {
    let mut gui = laid_out(dom);
    drawn(&mut gui, dom).iter().map(|q| q.color[0]).collect()
}

#[test]
fn z_index_behaviors_and_display_order() {
    let (mut dom, player_gui) = client();
    let s1 = child(&mut dom, "ScreenGui", player_gui, &[]);
    let p = square(&mut dom, s1, 5, 0.1);
    let q = square(&mut dom, s1, 3, 0.2);
    let pc = square(&mut dom, p, 1, 0.3);
    set(&mut dom, pc, "Position", udim2(0.0, 0, 0.0, 0));
    let qc = square(&mut dom, q, 10, 0.4);
    set(&mut dom, qc, "Position", udim2(0.0, 0, 0.0, 0));
    // Bottom first: the reverse of GetGuiObjectsAtPosition's lists.
    set_enum(&mut dom, s1, "ZIndexBehavior", "Global");
    assert_eq!(order(&mut dom), [0.3, 0.2, 0.1, 0.4]);
    set_enum(&mut dom, s1, "ZIndexBehavior", "Sibling");
    assert_eq!(order(&mut dom), [0.2, 0.4, 0.1, 0.3]);
    set(&mut dom, q, "ZIndex", Value::Int32(5));
    assert_eq!(order(&mut dom), [0.1, 0.3, 0.2, 0.4]);

    let s2 = child(&mut dom, "ScreenGui", player_gui, &[]);
    square(&mut dom, s2, 0, 0.5);
    assert_eq!(order(&mut dom).last(), Some(&0.5));
    set(&mut dom, s1, "DisplayOrder", Value::Int32(1));
    assert_eq!(order(&mut dom).first(), Some(&0.5));
}

#[test]
fn clipping_and_hidden_objects() {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let p = square(&mut dom, screen, 1, 0.1);
    set(&mut dom, p, "ClipsDescendants", Value::Bool(true));
    let c = square(&mut dom, p, 1, 0.2);
    set(&mut dom, c, "Position", udim2(0.0, 150, 0.0, 0));
    let hidden = square(&mut dom, screen, 1, 0.3);
    set(&mut dom, hidden, "Visible", Value::Bool(false));
    square(&mut dom, hidden, 1, 0.4);
    let mut gui = laid_out(&mut dom);
    let quads = drawn(&mut gui, &dom);
    assert_eq!(quads.len(), 2);
    assert_eq!(quads[0].clip, GuiQuad::NO_CLIP);
    assert_eq!(quads[1].clip, [600.0, 358.0, 800.0, 558.0]);

    // A turned clipper clips nothing (creator-docs ClipsDescendants).
    set(&mut dom, p, "Rotation", Value::Float32(10.0));
    let mut gui = laid_out(&mut dom);
    assert_eq!(drawn(&mut gui, &dom)[1].clip, GuiQuad::NO_CLIP);
}

#[test]
fn borders_go_around_the_background() {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    set_enum(&mut dom, screen, "ScreenInsets", "DeviceSafeInsets");
    let f = square(&mut dom, screen, 1, 0.1);
    set(&mut dom, f, "BorderSizePixel", Value::Int32(2));
    set(&mut dom, f, "BackgroundTransparency", Value::Float32(0.25));
    let mut gui = laid_out(&mut dom);
    let quads = drawn(&mut gui, &dom);
    // Four strips outside a 200 × 200 background, all 75% opaque.
    assert_eq!(quads.len(), 5);
    assert_eq!(quads[0].center, Vec2::new(700.0, 299.0));
    assert_eq!(quads[0].size, Vec2::new(204.0, 2.0));
    assert_eq!(quads[2].size, Vec2::new(2.0, 200.0));
    assert_eq!(quads[4].size, Vec2::new(200.0, 200.0));
    assert!(quads.iter().all(|q| q.color[3] == 0.75));
    set_enum(&mut dom, f, "BorderMode", "Inset");
    let mut gui = laid_out(&mut dom);
    assert_eq!(drawn(&mut gui, &dom)[4].size, Vec2::new(196.0, 196.0));
}

#[test]
fn hints_and_messages() {
    let (mut dom, _) = client();
    let ws = dom.find_service("Workspace").unwrap();
    let mut gui = laid_out(&mut dom);
    let empty_message = child(&mut dom, "Message", ws, &[]);
    gui.sync(&mut dom);
    assert!(
        drawn(&mut gui, &dom).is_empty(),
        "an empty Message shows nothing"
    );
    let hint = child(&mut dom, "Hint", ws, &[]);
    gui.sync(&mut dom);
    let quads = drawn(&mut gui, &dom);
    assert_eq!(quads.len(), 1, "an empty Hint shows its bar");
    assert_eq!(quads[0].color, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(quads[0].center.y, 10.0);
    // Made later, so drawn over the Hint.
    set(
        &mut dom,
        empty_message,
        "Text",
        Value::String(String::new()),
    );
    let message = child(&mut dom, "Message", ws, &[]);
    set(&mut dom, message, "Text", Value::String(" ".into()));
    gui.sync(&mut dom);
    let quads = drawn(&mut gui, &dom);
    assert_eq!(quads.last().unwrap().color[3], 0.5);
    dom.set_parent(hint, None).unwrap();
    gui.sync(&mut dom);
    assert_eq!(drawn(&mut gui, &dom).len(), 1);
    // Its text emptied, the Message shows nothing again.
    set(&mut dom, message, "Text", Value::String(String::new()));
    gui.sync(&mut dom);
    assert!(drawn(&mut gui, &dom).is_empty());
}

/// Screen text 34 pixels or bigger is drawn from distance fields, smaller
/// text from masks at its size (Inference from Studio's flags); needs the
/// install's fonts.
#[test]
fn big_screen_text_draws_from_distance_fields() {
    use rook_content::resolver::install::{Places, newest_from};
    let Some(install) = newest_from(None, &Places::from_env(|n| std::env::var_os(n))) else {
        return;
    };
    let mut config = rook_content::Config::empty();
    config.install = Some(install);
    config.network = false;
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let label = |dom: &mut Dom, size: f32| {
        let id = child(
            dom,
            "TextLabel",
            screen,
            &[
                ("Size", udim2(0.0, 400, 0.0, 100)),
                ("Text", Value::String("Ab".into())),
                ("TextSize", Value::Float32(size)),
                ("BackgroundTransparency", Value::Float32(1.0)),
            ],
        );
        set_enum(dom, id, "Font", "SourceSans");
        id
    };
    label(&mut dom, 20.0);
    let mut gui = Gui::default();
    gui.set_content(std::sync::Arc::new(rook_content::Resolver::new(config)));
    gui.sync(&mut dom);
    let fields = |quads: &[GuiQuad]| -> Vec<bool> {
        quads
            .iter()
            .filter_map(|q| match &q.paint {
                Paint::Glyph(m) => Some(m.distance),
                _ => None,
            })
            .collect()
    };
    assert_eq!(fields(&drawn(&mut gui, &dom)), [false, false]);
    label(&mut dom, 40.0);
    gui.sync(&mut dom);
    assert_eq!(fields(&drawn(&mut gui, &dom)), [false, false, true, true]);
}

/// A UICorner rounds the object's own quads and drops its border, a
/// UIGradient colors them, a UIStroke is a band over them, and a
/// CanvasGroup draws what it holds into its own quads, from its top left.
#[test]
fn corners_gradients_strokes_and_groups() {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let p = square(&mut dom, screen, 1, 0.1);
    set(&mut dom, p, "BorderSizePixel", Value::Int32(3));
    let corner = child(&mut dom, "UICorner", p, &[]);
    set(&mut dom, corner, "TopLeftRadius", udim(0.0, 500));
    child(&mut dom, "UIGradient", p, &[]);
    child(
        &mut dom,
        "UIStroke",
        p,
        &[("Thickness", Value::Float32(4.0))],
    );
    let mut gui = laid_out(&mut dom);
    let quads = drawn(&mut gui, &dom);
    assert_eq!(quads.len(), 2, "background and stroke, no border");
    let shape = quads[0].shape.expect("rounded");
    // Held to half the shorter side; the others keep the default 8.
    assert_eq!(shape.radii, [100.0, 8.0, 8.0, 8.0]);
    let g = quads[0].gradient.as_ref().expect("graded");
    assert_eq!(g.step, Vec2::new(1.0 / 200.0, 0.0));
    // Moved below the top bar with their quad, or the corner cuts a
    // shape 58 pixels above what it rounds.
    assert_eq!(shape.center, quads[0].center);
    assert_eq!(g.origin, quads[0].center);
    let band = quads[1].shape.expect("a band");
    assert_eq!((band.ring, band.size), (4.0, Vec2::splat(208.0)));
    assert_eq!(band.radii, [104.0, 12.0, 12.0, 12.0]);

    let group = child(
        &mut dom,
        "CanvasGroup",
        screen,
        &[
            ("Size", udim2(0.0, 100, 0.0, 50)),
            ("Position", udim2(0.0, 10, 0.0, 20)),
            ("GroupTransparency", Value::Float32(0.25)),
            ("BorderSizePixel", Value::Int32(0)),
        ],
    );
    let inner = square(&mut dom, group, 1, 0.5);
    set(&mut dom, inner, "Position", udim2(0.0, 5, 0.0, 5));
    let mut gui = laid_out(&mut dom);
    let quads = drawn(&mut gui, &dom);
    let Paint::Group(drawn) = &quads.last().unwrap().paint else {
        panic!("a group quad last");
    };
    assert_eq!(quads.last().unwrap().color[3], 0.75);
    assert_eq!(drawn.size, [100, 50]);
    assert_eq!(drawn.quads.len(), 2);
    assert_eq!(drawn.quads[1].center, Vec2::new(105.0, 105.0));
}

/// A change draws again only the pieces it touches: a color its object's,
/// a turn its object's and what it holds, showing a hidden object or a
/// layout change only what the ScreenGui's new stack moved. Every other
/// piece stays the same `Arc`, and the screen ends as a fresh drawing
/// draws it.
#[test]
fn changes_draw_again_only_their_pieces() {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let clipper = square(&mut dom, screen, 1, 0.1);
    set(&mut dom, clipper, "ClipsDescendants", Value::Bool(true));
    square(&mut dom, clipper, 1, 0.2);
    let other = square(&mut dom, screen, 2, 0.3);
    let hidden = square(&mut dom, screen, 3, 0.4);
    set(&mut dom, hidden, "Visible", Value::Bool(false));
    let mut gui = laid_out(&mut dom);
    let mut before = gui.draw(&dom);
    // Which pieces are those drawn last time, and whether the screen is
    // as a fresh drawing draws it.
    let mut step = |dom: &mut Dom, name: &str, value: Value, id: InstanceId| {
        set(dom, id, name, value);
        gui.sync(dom);
        let now = gui.draw(dom);
        let kept: Vec<bool> = (now.iter())
            .map(|p| before.iter().any(|b| Arc::ptr_eq(b, p)))
            .collect();
        before = now;
        assert_eq!(drawn(&mut gui, dom), drawn(&mut laid_out(dom), dom));
        kept
    };
    let red = Value::Color3(Color3::new(0.9, 0.0, 0.0));
    assert_eq!(
        step(&mut dom, "BackgroundColor3", red, other),
        [true, true, false]
    );
    let turn = Value::Float32(20.0);
    assert_eq!(
        step(&mut dom, "Rotation", turn, clipper),
        [false, false, true]
    );
    let shown = step(&mut dom, "Visible", Value::Bool(true), hidden);
    assert_eq!(shown, [true, true, true, false]);
    let size = udim2(0.0, 100, 0.0, 100);
    assert_eq!(
        step(&mut dom, "Size", size, hidden),
        [true, true, true, false]
    );
    // Laid out again, but only its border's look changed.
    let border = Value::Int32(3);
    let bordered = step(&mut dom, "BorderSizePixel", border, other);
    assert_eq!(bordered, [true, true, false, true]);
}

/// Hiding, showing or taking out an object a UIListLayout places: what's
/// hidden or gone stops drawing, the rows it moves draw again, and the
/// rest of the screen keeps its pieces, stacked by sibling (only the
/// list's run stacks again) or by ZIndex alone.
#[test]
fn hiding_in_a_list_restacks_only_the_list() {
    hiding_in_a_list("Sibling");
    hiding_in_a_list("Global");
}

fn hiding_in_a_list(behavior: &str) {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    set_enum(&mut dom, screen, "ZIndexBehavior", behavior);
    square(&mut dom, screen, 1, 0.1);
    let panel = child(
        &mut dom,
        "Frame",
        screen,
        &[("Size", udim2(0.0, 200, 0.0, 300))],
    );
    child(&mut dom, "UIListLayout", panel, &[]);
    let rows: Vec<InstanceId> = (0..3)
        .map(|i| {
            let color = Value::Color3(Color3::new(0.5 + i as f32 / 10.0, 0.5, 0.0));
            let size = udim2(1.0, 0, 0.0, 20);
            child(
                &mut dom,
                "Frame",
                panel,
                &[("Size", size), ("BackgroundColor3", color)],
            )
        })
        .collect();
    let mut gui = laid_out(&mut dom);
    let mut before = gui.draw(&dom);
    let mut step = |dom: &mut Dom, what: &str| {
        gui.sync(dom);
        let now = gui.draw(dom);
        let kept: Vec<bool> = (now.iter())
            .map(|p| before.iter().any(|b| Arc::ptr_eq(b, p)))
            .collect();
        before = now;
        let fresh = drawn(&mut laid_out(dom), dom);
        assert_eq!(drawn(&mut gui, dom), fresh, "{behavior}: {what}");
        kept
    };
    set(&mut dom, rows[1], "Visible", Value::Bool(false));
    assert_eq!(step(&mut dom, "hidden"), [true, true, true, false]);
    set(&mut dom, rows[1], "Visible", Value::Bool(true));
    assert_eq!(step(&mut dom, "shown"), [true, true, true, false, false]);
    // The panel it left draws again too (what leaves may be its UICorner).
    dom.set_parent(rows[0], None).unwrap();
    assert_eq!(step(&mut dom, "taken out"), [true, false, false, false]);
}

/// A ScreenGui coming or going, or GUI moving between ScreenGuis, lays
/// out and draws again only the trees it left and joined: the others'
/// pieces stay the same `Arc`, and the screen ends as a fresh drawing
/// draws it.
#[test]
fn trees_coming_and_going_leave_the_others() {
    let (mut dom, player_gui) = client();
    let kept = child(&mut dom, "ScreenGui", player_gui, &[]);
    square(&mut dom, kept, 1, 0.1);
    let other = child(&mut dom, "ScreenGui", player_gui, &[]);
    let mover = square(&mut dom, other, 1, 0.2);
    let mut gui = laid_out(&mut dom);
    let mut before = gui.draw(&dom);
    let mut step = |dom: &mut Dom, gui: &mut Gui| {
        gui.sync(dom);
        let now = gui.draw(dom);
        let kept: Vec<bool> = (now.iter())
            .map(|p| before.iter().any(|b| Arc::ptr_eq(b, p)))
            .collect();
        before = now;
        assert_eq!(drawn(gui, dom), drawn(&mut laid_out(dom), dom));
        kept
    };
    let added = dom.create_by_name("ScreenGui").unwrap();
    square(&mut dom, added, 1, 0.3);
    dom.set_parent(added, Some(player_gui)).unwrap();
    assert_eq!(step(&mut dom, &mut gui), [true, true, false]);
    // The square moved draws the same in its new ScreenGui: its piece too
    // is kept.
    dom.set_parent(mover, Some(added)).unwrap();
    assert_eq!(step(&mut dom, &mut gui), [true, true, true]);
    dom.set_parent(added, None).unwrap();
    assert_eq!(step(&mut dom, &mut gui), [true]);
    assert!(!gui.layout.contains(mover));
}

/// GUI added to, moved in or taken out of a list deep in a big ScreenGui
/// lays out only the list's panel again, not the ScreenGui, and the
/// screen ends as a fresh layout draws it: what left is gone, what came
/// shows, and a UICorner added rounds its parent.
#[test]
fn moves_inside_a_tree_lay_out_only_where_they_happen() {
    use rook_types::counters::{Counter, take_local};
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    for i in 0..40 {
        square(&mut dom, screen, 1, i as f32 / 100.0);
    }
    let panel = child(
        &mut dom,
        "Frame",
        screen,
        &[("Size", udim2(0.0, 200, 0.0, 300))],
    );
    child(&mut dom, "UIListLayout", panel, &[]);
    let item = |dom: &mut Dom, tag: f32| {
        let color = Value::Color3(Color3::new(tag, 0.5, 0.0));
        child(
            dom,
            "Frame",
            panel,
            &[
                ("Size", udim2(1.0, 0, 0.0, 20)),
                ("BackgroundColor3", color),
            ],
        )
    };
    let first = item(&mut dom, 0.9);
    let second = item(&mut dom, 0.8);
    let mut gui = laid_out(&mut dom);
    gui.draw(&dom);
    let mut step = |dom: &mut Dom, what: &str| {
        take_local();
        gui.sync(dom);
        let laid = take_local()[Counter::GuiNodesLaidOut];
        assert!(laid <= 6, "{what}: laid out {laid} nodes");
        assert_eq!(
            drawn(&mut gui, dom),
            drawn(&mut laid_out(dom), dom),
            "{what}"
        );
    };
    let added = dom.create_by_name("Frame").unwrap();
    set(&mut dom, added, "Size", udim2(1.0, 0, 0.0, 30));
    dom.set_parent(added, Some(panel)).unwrap();
    step(&mut dom, "added");
    dom.set_parent(first, None).unwrap();
    step(&mut dom, "taken out");
    let corner = dom.create_by_name("UICorner").unwrap();
    dom.set_parent(corner, Some(second)).unwrap();
    step(&mut dom, "corner");
}

/// A text object's color and transparency reach its glyphs, and change
/// only how it's drawn: the rest of its tree isn't laid out again. Needs
/// the install's fonts.
#[test]
fn text_colors_draw_again_without_laying_out() {
    use rook_content::resolver::install::{Places, newest_from};
    let Some(install) = newest_from(None, &Places::from_env(|n| std::env::var_os(n))) else {
        return;
    };
    let mut config = rook_content::Config::empty();
    config.install = Some(install);
    config.network = false;
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    for _ in 0..20 {
        square(&mut dom, screen, 1, 0.1);
    }
    let label = child(
        &mut dom,
        "TextLabel",
        screen,
        &[
            ("Size", udim2(0.0, 400, 0.0, 100)),
            ("Text", Value::String("Ab".into())),
            ("TextSize", Value::Float32(20.0)),
            ("BackgroundTransparency", Value::Float32(1.0)),
        ],
    );
    let mut gui = Gui::default();
    gui.set_content(std::sync::Arc::new(rook_content::Resolver::new(
        config.clone(),
    )));
    gui.sync(&mut dom);
    let glyphs = |quads: &[GuiQuad]| -> Vec<[f32; 4]> {
        (quads.iter())
            .filter(|q| matches!(q.paint, Paint::Glyph(_)))
            .map(|q| q.color)
            .collect()
    };
    assert_eq!(glyphs(&drawn(&mut gui, &dom)).len(), 2);
    rook_types::counters::take_local();
    set(
        &mut dom,
        label,
        "TextColor3",
        Value::Color3(Color3::new(1.0, 0.0, 0.0)),
    );
    set(&mut dom, label, "TextTransparency", Value::Float32(0.5));
    set(
        &mut dom,
        label,
        "TextStrokeTransparency",
        Value::Float32(0.0),
    );
    gui.sync(&mut dom);
    assert_eq!(glyphs(&drawn(&mut gui, &dom)), [[1.0, 0.0, 0.0, 0.5]; 2]);
    let counts = rook_types::counters::take_local();
    assert_eq!(counts[rook_types::counters::Counter::GuiNodesLaidOut], 0);
    // Recolored where it was laid out, outline and all, as a fresh
    // layout draws it (its own glyph masks apart).
    let mut fresh = Gui::default();
    fresh.set_content(std::sync::Arc::new(rook_content::Resolver::new(config)));
    fresh.sync(&mut dom);
    let looks = |quads: Vec<GuiQuad>| -> Vec<_> {
        (quads.into_iter())
            .map(|q| {
                (
                    q.center,
                    q.size,
                    q.color,
                    q.clip,
                    std::mem::discriminant(&q.paint),
                )
            })
            .collect()
    };
    assert_eq!(looks(drawn(&mut gui, &dom)), looks(drawn(&mut fresh, &dom)));
}

/// Incremental drawing equals drawing from scratch, through a seeded run of
/// the changes a tree-view script (Dark Dex's explorer) makes to recycled
/// rows in a ScrollingFrame: icons moved by scale inside clipping frames,
/// rows and arrows shown and hidden, rows and indents moved, rows
/// restyled, restacked and taken out and put back, the list scrolled.
#[test]
fn incremental_draws_match_fresh_ones() {
    for behavior in ["Global", "Sibling"] {
        incremental_draws(behavior, 0x2545_f491, 400);
    }
}

fn incremental_draws(behavior: &str, mut seed: u32, steps: u32) {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    set_enum(&mut dom, screen, "ZIndexBehavior", behavior);
    let list = child(
        &mut dom,
        "ScrollingFrame",
        screen,
        &[
            ("Size", udim2(0.0, 300, 0.0, 200)),
            ("CanvasSize", udim2(0.0, 0, 0.0, 400)),
        ],
    );
    let mut rows = Vec::new();
    let mut icons = Vec::new();
    let mut arrows = Vec::new();
    let mut indents = Vec::new();
    for i in 0..6 {
        let row = frame(&mut dom, list, udim2(1.0, 0, 0.0, 20));
        set(&mut dom, row, "Position", udim2(0.0, 0, 0.0, i * 20));
        let indent = frame(&mut dom, row, udim2(1.0, 0, 1.0, 0));
        set(
            &mut dom,
            indent,
            "BackgroundTransparency",
            Value::Float32(1.0),
        );
        for (x, out) in [(0, &mut arrows), (18, &mut icons)] {
            let clip = frame(&mut dom, indent, udim2(0.0, 16, 0.0, 16));
            set(&mut dom, clip, "Position", udim2(0.0, x, 0.0, 2));
            set(&mut dom, clip, "ClipsDescendants", Value::Bool(true));
            let map = frame(&mut dom, clip, udim2(16.0, 0, 16.0, 0));
            out.push((clip, map));
        }
        rows.push(row);
        indents.push(indent);
    }
    let mut gui = laid_out(&mut dom);
    let mut next = |n: u32| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed % n
    };
    for step in 0..steps {
        for _ in 0..1 + next(4) {
            let r = next(6) as usize;
            match next(9) {
                0 => {
                    let (col, row) = (next(14) as f32, next(14) as f32);
                    let map = [icons[r].1, arrows[r].1][next(2) as usize];
                    set(
                        &mut dom,
                        map,
                        "Position",
                        udim2(-col - 0.2, 0, -row - 0.2, 0),
                    );
                }
                1 => {
                    let shown = next(2) == 0;
                    set(&mut dom, rows[r], "Visible", Value::Bool(shown));
                }
                2 => {
                    let shown = next(2) == 0;
                    set(&mut dom, arrows[r].0, "Visible", Value::Bool(shown));
                }
                3 => {
                    let x = next(4) as i32 * 18;
                    set(&mut dom, indents[r], "Position", udim2(0.0, x, 0.0, 0));
                }
                4 => {
                    let t = next(2) as f32;
                    set(
                        &mut dom,
                        rows[r],
                        "BackgroundTransparency",
                        Value::Float32(t),
                    );
                }
                5 => {
                    let y = next(8) as i32 * 20;
                    set(&mut dom, rows[r], "Position", udim2(0.0, 0, 0.0, y));
                }
                6 => {
                    let z = Value::Int32(1 + next(3) as i32);
                    set(&mut dom, icons[r].0, "ZIndex", z);
                }
                7 => {
                    let y = next(10) as f32 * 20.0;
                    set(&mut dom, list, "CanvasPosition", v2(0.0, y));
                }
                _ => {
                    // A row out of the list and back, or in again.
                    let parent = [None, Some(list)][next(2) as usize];
                    dom.set_parent(rows[r], parent).unwrap();
                }
            }
        }
        gui.sync(&mut dom);
        let incremental = drawn(&mut gui, &dom);
        let mut fresh = laid_out(&mut dom);
        assert_eq!(
            incremental,
            drawn(&mut fresh, &dom),
            "{behavior} step {step}"
        );
    }
}

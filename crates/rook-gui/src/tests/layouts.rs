use super::*;

#[test]
fn screen_size_inset_and_anchor_point() {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let whole = child(&mut dom, "ScreenGui", player_gui, &[]);
    set_enum(&mut dom, whole, "ScreenInsets", "DeviceSafeInsets");
    let f = frame(&mut dom, screen, udim2(0.5, 10, 0.25, -4));
    set(&mut dom, f, "Position", udim2(0.1, 3, 0.2, 5));
    set(&mut dom, f, "AnchorPoint", v2(0.5, 1.0));
    laid_out(&mut dom);
    assert_eq!(
        vec2_of(&dom, screen, "AbsoluteSize"),
        Vec2::new(1561.0, 735.0)
    );
    assert_eq!(
        vec2_of(&dom, whole, "AbsoluteSize"),
        Vec2::new(1561.0, 793.0)
    );
    near(rect(&dom, f, screen), ([-236.15, -27.75], [790.5, 179.75]));
}

#[test]
fn folders_size_constraint_and_negative_sizes() {
    let (mut dom, b) = boxed();
    let folder = child(&mut dom, "Folder", b, &[]);
    let c = frame(&mut dom, folder, udim2(0.5, 0, 0.5, 0));
    set(&mut dom, c, "Position", udim2(0.5, 0, 0.0, 0));
    let yy = frame(&mut dom, b, udim2(0.5, 0, 0.5, 0));
    set_enum(&mut dom, yy, "SizeConstraint", "RelativeYY");
    let xx = frame(&mut dom, b, udim2(0.5, 0, 0.5, 0));
    set_enum(&mut dom, xx, "SizeConstraint", "RelativeXX");
    set(&mut dom, xx, "Position", udim2(0.5, 0, 0.5, 0));
    let negative = frame(&mut dom, b, udim2(0.0, -100, 0.0, 50));
    laid_out(&mut dom);
    near(rect(&dom, c, b), ([200.0, 0.0], [200.0, 150.0]));
    near(rect(&dom, yy, b), ([0.0, 0.0], [150.0, 150.0]));
    near(rect(&dom, xx, b), ([200.0, 150.0], [200.0, 200.0]));
    near(rect(&dom, negative, b), ([0.0, 0.0], [-100.0, 50.0]));
}

#[test]
fn rotation_turns_children_about_the_center() {
    let (mut dom, b) = boxed();
    let r = frame(&mut dom, b, udim2(0.0, 100, 0.0, 50));
    set(&mut dom, r, "Position", udim2(0.0, 100, 0.0, 100));
    set(&mut dom, r, "Rotation", Value::Float32(90.0));
    let c = frame(&mut dom, r, udim2(0.0, 10, 0.0, 10));
    set(&mut dom, c, "Rotation", Value::Float32(15.0));
    laid_out(&mut dom);
    near(rect(&dom, c, b), ([165.0, 75.0], [10.0, 10.0]));
    assert_eq!(dom.float_property(c, "AbsoluteRotation"), Some(105.0));
    set(&mut dom, r, "AnchorPoint", v2(1.0, 1.0));
    laid_out(&mut dom);
    near(rect(&dom, c, b), ([65.0, 25.0], [10.0, 10.0]));
}

/// Three items like Studio's: `A` (anchored, positioned, both ignored),
/// `B` half the box wide, `C` last by LayoutOrder.
fn list_case() -> (Dom, InstanceId, InstanceId, [InstanceId; 3]) {
    let (mut dom, b) = boxed();
    let list = child(&mut dom, "UIListLayout", b, &[]);
    set_enum(&mut dom, list, "SortOrder", "LayoutOrder");
    let size = udim2(0.0, 50, 0.0, 40);
    let c = child(
        &mut dom,
        "Frame",
        b,
        &[("Size", size.clone()), ("LayoutOrder", Value::Int32(2))],
    );
    let a = child(
        &mut dom,
        "Frame",
        b,
        &[
            ("Size", size),
            ("LayoutOrder", Value::Int32(1)),
            ("Position", udim2(0.0, 99, 0.0, 99)),
            ("AnchorPoint", v2(1.0, 1.0)),
        ],
    );
    let bb = child(
        &mut dom,
        "Frame",
        b,
        &[
            ("Size", udim2(0.5, 0, 0.1, 0)),
            ("LayoutOrder", Value::Int32(1)),
        ],
    );
    set(&mut dom, list, "Padding", udim(0.1, 5));
    (dom, b, list, [a, bb, c])
}

#[test]
fn list_layout() {
    let (mut dom, b, list, [a, bb, c]) = list_case();
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([0.0, 0.0], [50.0, 40.0]));
    near(rect(&dom, bb, b), ([0.0, 75.0], [200.0, 30.0]));
    near(rect(&dom, c, b), ([0.0, 140.0], [50.0, 40.0]));
    assert_eq!(
        vec2_of(&dom, list, "AbsoluteContentSize"),
        Vec2::new(200.0, 180.0)
    );

    set_enum(&mut dom, list, "HorizontalAlignment", "Center");
    set_enum(&mut dom, list, "VerticalAlignment", "Center");
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([175.0, 60.0], [50.0, 40.0]));
    near(rect(&dom, bb, b), ([100.0, 135.0], [200.0, 30.0]));

    set_enum(&mut dom, list, "HorizontalAlignment", "Right");
    set_enum(&mut dom, list, "VerticalAlignment", "Bottom");
    set_enum(&mut dom, list, "FillDirection", "Horizontal");
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([10.0, 260.0], [50.0, 40.0]));
    near(rect(&dom, bb, b), ([105.0, 270.0], [200.0, 30.0]));
    near(rect(&dom, c, b), ([350.0, 260.0], [50.0, 40.0]));
    assert_eq!(
        vec2_of(&dom, list, "AbsoluteContentSize"),
        Vec2::new(390.0, 40.0)
    );
}

#[test]
fn hidden_list_items_keep_their_place() {
    let (mut dom, b, list, [a, bb, c]) = list_case();
    set_enum(&mut dom, list, "FillDirection", "Horizontal");
    set_enum(&mut dom, list, "HorizontalAlignment", "Right");
    set_enum(&mut dom, list, "VerticalAlignment", "Bottom");
    let mut gui = laid_out(&mut dom);
    set_enum(&mut dom, list, "HorizontalAlignment", "Left");
    set_enum(&mut dom, list, "VerticalAlignment", "Top");
    set(&mut dom, c, "Visible", Value::Bool(false));
    gui.sync(&mut dom);
    near(rect(&dom, c, b), ([350.0, 260.0], [50.0, 40.0]));
    near(rect(&dom, a, b), ([0.0, 0.0], [50.0, 40.0]));
    near(rect(&dom, bb, b), ([95.0, 0.0], [200.0, 30.0]));
    assert_eq!(
        vec2_of(&dom, list, "AbsoluteContentSize"),
        Vec2::new(295.0, 40.0)
    );
}

#[test]
fn padding_with_and_without_a_list() {
    let (mut dom, b, list, [a, bb, c]) = list_case();
    set_enum(&mut dom, list, "FillDirection", "Horizontal");
    set_enum(&mut dom, list, "HorizontalAlignment", "Right");
    set(&mut dom, bb, "Size", udim2(0.0, 500, 0.0, 10));
    child(
        &mut dom,
        "UIPadding",
        b,
        &[
            ("PaddingLeft", udim(0.0, 7)),
            ("PaddingTop", udim(0.1, 0)),
            ("PaddingRight", udim(0.0, 13)),
            ("PaddingBottom", udim(0.0, 3)),
        ],
    );
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([-299.0, 30.0], [50.0, 40.0]));
    near(rect(&dom, c, b), ([337.0, 30.0], [50.0, 40.0]));
    assert_eq!(
        vec2_of(&dom, list, "AbsoluteContentSize"),
        Vec2::new(686.0, 40.0)
    );

    dom.destroy(list).unwrap();
    set(&mut dom, a, "Size", udim2(0.5, 0, 0.5, 0));
    set(&mut dom, a, "Position", udim2(0.5, 0, 0.0, 0));
    set(&mut dom, a, "AnchorPoint", v2(0.0, 0.0));
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([197.0, 30.0], [190.0, 133.5]));
}

#[test]
fn grid_layout() {
    let (mut dom, b) = boxed();
    let grid = child(&mut dom, "UIGridLayout", b, &[]);
    let kids: Vec<InstanceId> = (1..=5)
        .map(|i| {
            let k = frame(&mut dom, b, udim2(0.0, 50, 0.0, 40));
            dom.set_name(k, format!("K{i}")).unwrap();
            k
        })
        .collect();
    set_enum(&mut dom, grid, "SortOrder", "Name");
    laid_out(&mut dom);
    near(rect(&dom, kids[4], b), ([105.0, 105.0], [100.0, 100.0]));
    assert_eq!(
        vec2_of(&dom, grid, "AbsoluteCellCount"),
        Vec2::new(3.0, 2.0)
    );
    assert_eq!(
        vec2_of(&dom, grid, "AbsoluteContentSize"),
        Vec2::new(310.0, 205.0)
    );

    set(&mut dom, grid, "CellSize", udim2(0.3, 0, 0.0, 50));
    set(&mut dom, grid, "CellPadding", udim2(0.0, 10, 0.1, 0));
    set(&mut dom, grid, "FillDirectionMaxCells", Value::Int32(2));
    set_enum(&mut dom, grid, "StartCorner", "BottomRight");
    laid_out(&mut dom);
    near(rect(&dom, kids[0], b), ([130.0, 160.0], [120.0, 50.0]));
    near(rect(&dom, kids[1], b), ([0.0, 160.0], [120.0, 50.0]));
    near(rect(&dom, kids[4], b), ([130.0, 0.0], [120.0, 50.0]));
    // Studio's cell is 120.000008 wide too.
    assert!(vec2_of(&dom, grid, "AbsoluteContentSize").abs_diff_eq(Vec2::new(250.0, 210.0), 1e-3));

    set_enum(&mut dom, grid, "StartCorner", "TopLeft");
    set_enum(&mut dom, grid, "FillDirection", "Vertical");
    set(&mut dom, grid, "FillDirectionMaxCells", Value::Int32(0));
    set_enum(&mut dom, grid, "HorizontalAlignment", "Center");
    set_enum(&mut dom, grid, "VerticalAlignment", "Center");
    laid_out(&mut dom);
    near(rect(&dom, kids[3], b), ([75.0, 245.0], [120.0, 50.0]));
    near(rect(&dom, kids[4], b), ([205.0, 5.0], [120.0, 50.0]));
    assert_eq!(
        vec2_of(&dom, grid, "AbsoluteCellCount"),
        Vec2::new(2.0, 4.0)
    );
}

#[test]
fn size_and_aspect_constraints() {
    let (mut dom, b) = boxed();
    let f = frame(&mut dom, b, udim2(0.5, 0, 0.5, 0));
    set(&mut dom, f, "AnchorPoint", v2(0.5, 0.5));
    set(&mut dom, f, "Position", udim2(0.5, 0, 0.5, 0));
    let sc = child(
        &mut dom,
        "UISizeConstraint",
        f,
        &[("MinSize", v2(10.0, 160.0)), ("MaxSize", v2(120.0, 1000.0))],
    );
    laid_out(&mut dom);
    near(rect(&dom, f, b), ([140.0, 70.0], [120.0, 160.0]));
    set(&mut dom, sc, "MinSize", v2(200.0, 0.0));
    set(&mut dom, sc, "MaxSize", v2(100.0, 100.0));
    laid_out(&mut dom);
    near(rect(&dom, f, b), ([100.0, 75.0], [200.0, 150.0]));
    dom.destroy(sc).unwrap();

    let ar = child(
        &mut dom,
        "UIAspectRatioConstraint",
        f,
        &[("AspectRatio", Value::Float32(0.5))],
    );
    laid_out(&mut dom);
    near(rect(&dom, f, b), ([162.5, 75.0], [75.0, 150.0]));
    set_enum(&mut dom, ar, "AspectType", "ScaleWithParentSize");
    laid_out(&mut dom);
    near(rect(&dom, f, b), ([125.0, 0.0], [150.0, 300.0]));
    set(&mut dom, ar, "AspectRatio", Value::Float32(4.0));
    set_enum(&mut dom, ar, "DominantAxis", "Height");
    laid_out(&mut dom);
    near(rect(&dom, f, b), ([0.0, 100.0], [400.0, 100.0]));
    set_enum(&mut dom, ar, "AspectType", "FitWithinMaxSize");
    set(&mut dom, ar, "AspectRatio", Value::Float32(0.5));
    child(
        &mut dom,
        "UISizeConstraint",
        f,
        &[("MaxSize", v2(1000.0, 60.0))],
    );
    laid_out(&mut dom);
    near(rect(&dom, f, b), ([185.0, 120.0], [30.0, 60.0]));
}

#[test]
fn ui_scale_scales_about_the_anchor_point() {
    let (mut dom, b) = boxed();
    let f = frame(&mut dom, b, udim2(0.5, 0, 0.5, 0));
    set(&mut dom, f, "AnchorPoint", v2(0.5, 0.5));
    set(&mut dom, f, "Position", udim2(0.5, 0, 0.5, 0));
    let c = frame(&mut dom, f, udim2(0.5, 0, 0.5, 0));
    set(&mut dom, c, "Position", udim2(0.5, 0, 0.0, 0));
    let s = child(&mut dom, "UIScale", f, &[("Scale", Value::Float32(2.0))]);
    laid_out(&mut dom);
    near(rect(&dom, f, b), ([0.0, 0.0], [400.0, 300.0]));
    near(rect(&dom, c, b), ([200.0, 0.0], [200.0, 150.0]));
    dom.set_parent(s, Some(b)).unwrap();
    laid_out(&mut dom);
    assert_eq!(vec2_of(&dom, b, "AbsoluteSize"), Vec2::new(800.0, 600.0));
    near(rect(&dom, c, b), ([400.0, 150.0], [200.0, 150.0]));
    let screen = dom.parent(b).unwrap();
    dom.set_parent(s, Some(screen)).unwrap();
    laid_out(&mut dom);
    near(rect(&dom, b, screen), ([20.0, 40.0], [800.0, 600.0]));
    assert_eq!(
        vec2_of(&dom, screen, "AbsoluteSize"),
        Vec2::new(1561.0, 735.0)
    );
}

#[test]
fn automatic_size() {
    let (mut dom, b) = boxed();
    let a = frame(&mut dom, b, udim2(0.0, 20, 0.0, 10));
    set(&mut dom, a, "Position", udim2(0.0, 5, 0.0, 5));
    set_enum(&mut dom, a, "AutomaticSize", "XY");
    let a1 = frame(&mut dom, a, udim2(0.0, 100, 0.0, 30));
    set(&mut dom, a1, "Position", udim2(0.0, 10, 0.0, 50));
    let a2 = frame(&mut dom, a, udim2(0.5, 0, 0.5, 0));
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([5.0, 5.0], [110.0, 80.0]));
    near(rect(&dom, a2, b), ([5.0, 5.0], [55.0, 40.0]));
    set_enum(&mut dom, a, "AutomaticSize", "X");
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([5.0, 5.0], [110.0, 10.0]));
    set_enum(&mut dom, a, "AutomaticSize", "XY");
    set(&mut dom, a1, "AnchorPoint", v2(1.0, 0.0));
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([5.0, 5.0], [20.0, 80.0]));
    set(&mut dom, a1, "AnchorPoint", v2(0.0, 0.0));
    set(&mut dom, a2, "Size", udim2(1.0, 0, 0.0, 5));
    let padding = child(
        &mut dom,
        "UIPadding",
        a,
        &[
            ("PaddingLeft", udim(0.0, 4)),
            ("PaddingRight", udim(0.0, 6)),
            ("PaddingTop", udim(0.0, 2)),
        ],
    );
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([5.0, 5.0], [120.0, 82.0]));
    near(rect(&dom, a1, b), ([19.0, 57.0], [100.0, 30.0]));
    dom.destroy(padding).unwrap();

    // With a list, children sized by scale fill the room left in the
    // parent.
    set(&mut dom, a, "Position", udim2(0.1, 5, 0.0, 5));
    set(&mut dom, a1, "Size", udim2(0.0, 30, 0.0, 30));
    let list = child(&mut dom, "UIListLayout", a, &[]);
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([45.0, 5.0], [355.0, 35.0]));
    near(rect(&dom, a2, b), ([45.0, 35.0], [355.0, 5.0]));
    set(&mut dom, a2, "Size", udim2(1.0, -20, 0.0, 5));
    laid_out(&mut dom);
    near(rect(&dom, a, b), ([45.0, 5.0], [335.0, 35.0]));
    assert_eq!(
        vec2_of(&dom, list, "AbsoluteContentSize"),
        Vec2::new(315.0, 35.0)
    );
}

/// Measured: an Inset border is taken off every side of the area inside,
/// a Middle one by half, as a UIPadding would be.
#[test]
fn inset_borders_pad_what_is_inside() {
    let (mut dom, frame) = boxed();
    let f = child(
        &mut dom,
        "Frame",
        frame,
        &[
            ("Size", udim2(0.0, 200, 0.0, 100)),
            ("BorderSizePixel", Value::Int32(4)),
        ],
    );
    let c = child(
        &mut dom,
        "Frame",
        f,
        &[
            ("Position", udim2(0.0, 10, 0.0, 20)),
            ("Size", udim2(0.5, 0, 0.5, 0)),
        ],
    );
    for (mode, want) in [
        ("Outline", ([10.0, 20.0], [100.0, 50.0])),
        ("Inset", ([14.0, 24.0], [96.0, 46.0])),
        ("Middle", ([12.0, 22.0], [98.0, 48.0])),
    ] {
        set_enum(&mut dom, f, "BorderMode", mode);
        laid_out(&mut dom);
        near(rect(&dom, c, f), want);
    }
}

#[test]
fn nested_automatic_sizes() {
    let (mut dom, b) = boxed();
    let outer = frame(&mut dom, b, udim2(0.0, 0, 0.0, 0));
    set_enum(&mut dom, outer, "AutomaticSize", "XY");
    let inner = frame(&mut dom, outer, udim2(0.0, 0, 0.0, 0));
    set_enum(&mut dom, inner, "AutomaticSize", "XY");
    set(&mut dom, inner, "Position", udim2(0.0, 5, 0.0, 5));
    let leaf = frame(&mut dom, inner, udim2(0.0, 30, 0.0, 20));
    set(&mut dom, leaf, "Position", udim2(0.0, 7, 0.0, 9));
    laid_out(&mut dom);
    near(rect(&dom, outer, b), ([0.0, 0.0], [42.0, 34.0]));
    near(rect(&dom, inner, b), ([5.0, 5.0], [37.0, 29.0]));
}

#[test]
fn reads_lay_out_unparented_trees() {
    let (mut dom, _) = client();
    let u = dom.create_by_name("Frame").unwrap();
    set(&mut dom, u, "Size", udim2(0.0, 50, 0.0, 40));
    let c = frame(&mut dom, u, udim2(0.5, 0, 0.5, 0));
    let mut gui = Gui::default();
    gui.sync_for(&mut dom, c);
    assert_eq!(vec2_of(&dom, u, "AbsoluteSize"), Vec2::new(50.0, 40.0));
    assert_eq!(vec2_of(&dom, c, "AbsoluteSize"), Vec2::new(25.0, 20.0));
    assert_eq!(vec2_of(&dom, c, "AbsolutePosition"), Vec2::ZERO);
}

#[test]
fn only_gui_changes_lay_out_again() {
    let (mut dom, b) = boxed();
    let mut gui = laid_out(&mut dom);
    let first = gui.generation();
    let ws = dom.find_service("Workspace").unwrap();
    child(&mut dom, "Part", ws, &[]);
    gui.sync(&mut dom);
    assert_eq!(gui.generation(), first);
    set(&mut dom, b, "Size", udim2(0.0, 10, 0.0, 10));
    gui.sync(&mut dom);
    assert_eq!(gui.generation(), first + 1);
    assert_eq!(vec2_of(&dom, b, "AbsoluteSize"), Vec2::new(10.0, 10.0));
}

/// An object moved, resized or turned is placed again on its own, or with
/// its whole tree when a UI layout places it, ending where a whole layout
/// puts everything.
#[test]
fn objects_placed_again_alone_match_a_whole_layout() {
    let (mut dom, b, _, [a, bb, c]) = list_case();
    let r = frame(&mut dom, a, udim2(0.0, 20, 0.0, 10));
    let inner = frame(&mut dom, r, udim2(0.5, 0, 0.5, 0));
    let mut gui = laid_out(&mut dom);
    set(&mut dom, r, "Rotation", Value::Float32(30.0));
    set(&mut dom, r, "Position", udim2(0.5, 3, 0.0, 7));
    set(&mut dom, bb, "Size", udim2(0.0, 70, 0.0, 15));
    gui.sync(&mut dom);
    let ids = [a, bb, c, r, inner];
    let read =
        |dom: &Dom| ids.map(|id| (rect(dom, id, b), dom.float_property(id, "AbsoluteRotation")));
    let alone = read(&dom);
    laid_out(&mut dom);
    assert_eq!(alone, read(&dom));
    assert_eq!(alone[4].1, Some(30.0));
}

use super::*;

#[test]
fn verbose_takes_a_subsystem_but_not_a_file() {
    let args = |a: &[&str]| fix_verbose_args(a.iter().map(std::string::ToString::to_string));
    assert_eq!(
        args(&["rook-player", "--verbose", "place.rbxl"]),
        ["rook-player", "--verbose", "place.rbxl"]
    );
    assert_eq!(
        args(&["rook-player", "--verbose", "content", "place.rbxl"]),
        ["rook-player", "--verbose=content", "place.rbxl"]
    );
    let content = Filter::parse("content", Level::INFO).unwrap();
    assert!(content.shows("rook_content::loader", Level::TRACE));
    assert!(!content.shows("rook_terrain", Level::DEBUG));
    assert!(!content.shows("wgpu_core", Level::INFO));
}

/// A log file that keeps what's written, for reading back.
#[derive(Clone, Default)]
struct Captured(std::sync::Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn lines_reach_the_log_file_in_order_by_a_flush() {
    let file = Captured::default();
    SINK.lock().unwrap().file = Some(LineWriter::new(Box::new(file.clone())));
    for i in 0..1000 {
        crate::console::print_rook(rook_lua::MessageType::Output, &format!("order {i}"), false);
    }
    crate::console::flush();
    let text = String::from_utf8(file.0.lock().unwrap().clone()).unwrap();
    let numbers: Vec<usize> = text
        .lines()
        .filter_map(|l| {
            l.split("order ")
                .nth(1)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
        .collect();
    assert_eq!(numbers, (0..1000).collect::<Vec<_>>());
    assert!(!text.contains('\u{1b}'), "the file has no color codes");
    SINK.lock().unwrap().file = None;
}

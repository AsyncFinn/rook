//! Builds the shaders: the WESL modules in `src/shaders` linked into one
//! WGSL source per entry module, for each way draws reach their images
//! (`image_table` on or off). Each source is checked with naga here, and
//! an error is reported at the file and line it came from.

use std::error::Error;
use std::ops::Range;

use wesl::sourcemap::{BasicSourceMap, SourceMap};
use wesl::{CompileOptions, CompileResult, Compiler};

/// The entry modules: one shader module each.
const ENTRIES: [&str; 4] = ["parts", "sky", "light_volume", "particles"];

fn main() {
    for entry in ENTRIES {
        for table in [false, true] {
            let mut options = CompileOptions::default();
            options.features.set("image_table", table);
            let result = Compiler::new(options)
                .compile(format!("src/shaders/{entry}.wgsl"))
                .unwrap_or_else(|e| panic!("{e}"));
            result.emit_rerun_if_changed();
            check(&result, entry);
            let artifact = if table {
                format!("{entry}_table")
            } else {
                entry.to_string()
            };
            result.write_artifact(&artifact);
        }
    }
}

/// Parses and validates the linked WGSL with naga (the version wgpu
/// uses), panicking with where in the shader sources the error is.
fn check(result: &CompileResult, entry: &str) {
    let wgsl = result.to_string();
    let (message, range) = match naga::front::wgsl::parse_str(&wgsl) {
        Err(e) => (
            e.message().to_string(),
            e.labels().find_map(|(span, _)| span.to_range()),
        ),
        Ok(module) => {
            let validated = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module);
            let Err(e) = validated else {
                return;
            };
            // The innermost span is the most precise.
            let range = e.spans().filter_map(|(span, _)| span.to_range()).last();
            let mut message = e.as_inner().to_string();
            let mut source = e.as_inner().source();
            while let Some(s) = source {
                message += &format!(": {s}");
                source = s.source();
            }
            (message, range)
        }
    };
    let at = range
        .and_then(|r| original_location(result, &wgsl, r))
        .unwrap_or_else(|| format!("{entry} (linked; no source location)"));
    panic!("{at}: {message}");
}

/// Where the linked WGSL at `range` came from, as `file:line`: the
/// declaration holding it, found through the sourcemap, then its line
/// within that declaration's source (else the declaration's first line).
/// wesl prints its syntax tree, not the original text, so lines are found
/// by their text with the mangled names turned back.
fn original_location(result: &CompileResult, wgsl: &str, range: Range<usize>) -> Option<String> {
    let sourcemap = result.sourcemap.as_ref()?;
    let line_end = wgsl[range.start..]
        .find('\n')
        .map_or(wgsl.len(), |e| range.start + e);
    let line_start = wgsl[..range.start].rfind('\n').map_or(0, |s| s + 1);
    let name = declaration_at(&wgsl[..line_end])?;
    // The entry module's declarations keep their names; the rest are
    // mangled, and the sourcemap knows them.
    let (module, original) = match sourcemap.item(&name) {
        Some(item) => (item.path.clone(), item.name.clone()),
        None => (result.modules.first()?.path.clone(), name),
    };
    let file = sourcemap.file(&module)?;
    let source = &file.source;
    let snippets = [
        demangle(sourcemap, wgsl[line_start..line_end].trim()),
        demangle(sourcemap, wgsl[range].trim()),
    ];
    // The `@if` and `@else` variants of a declaration share its name: the
    // first holding the line, else the first holding the span's text.
    let regions: Vec<_> = declaration_starts(source, &original)
        .map(|start| {
            let end = source[start..]
                .find("\n}")
                .map_or(source.len(), |e| start + e);
            start..end
        })
        .collect();
    let at = snippets
        .iter()
        .filter(|s| !s.is_empty())
        .find_map(|snippet| {
            regions.iter().find_map(|r| {
                source[r.clone()]
                    .find(snippet.as_str())
                    .map(|i| r.start + i)
            })
        })
        .or_else(|| regions.first().map(|r| r.start))?;
    let line = source[..at].matches('\n').count() + 1;
    let path = file.path.as_ref()?;
    Some(format!("{}:{line}", path.display()))
}

/// `text` with each mangled name turned back into the name it had.
fn demangle(sourcemap: &BasicSourceMap, text: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
            continue;
        }
        match sourcemap.item(&word) {
            Some(item) => out += &item.name,
            None => out += &word,
        }
        word.clear();
        out.push(c);
    }
    out.pop();
    out
}

/// The name of the top-level declaration the end of `before` is in.
fn declaration_at(before: &str) -> Option<String> {
    let keywords = ["fn", "struct", "const", "var", "override", "alias"];
    before
        .lines()
        .rev()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .find_map(|line| {
            // Attributes first: `@group(0) @binding(0) var<uniform> g`.
            let mut rest = line.trim_start();
            while let Some(attribute) = rest.strip_prefix('@') {
                rest = attribute.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
                if rest.starts_with('(') {
                    rest = &rest[rest.find(')')? + 1..];
                }
                rest = rest.trim_start();
            }
            let keyword = keywords.iter().find(|k| {
                rest.strip_prefix(**k)
                    .is_some_and(|r| r.starts_with(|c: char| c.is_whitespace() || c == '<'))
            })?;
            let mut rest = &rest[keyword.len()..];
            if rest.starts_with('<') {
                rest = &rest[rest.find('>')? + 1..];
            }
            let name: String = rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
}

/// The starts of the lines where `name` is declared in a module's source.
fn declaration_starts<'a>(source: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    source.match_indices(name).filter_map(move |(i, _)| {
        let before = source[..i].trim_end();
        let after = source[i + name.len()..].chars().next()?;
        let declares = ["fn", "struct", "const", "var", "override", "alias"]
            .iter()
            .any(|k| before.ends_with(k))
            || before.ends_with('>');
        (declares && !after.is_alphanumeric() && after != '_')
            .then(|| source[..i].rfind('\n').map_or(0, |n| n + 1))
    })
}

use std::path::Path;

use anyhow::Context;
use rbx_dom_weak::{Instance, WeakDom};

pub fn run(file: &Path, properties: bool, depth: Option<usize>) -> anyhow::Result<()> {
    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let loaded = raven_formats::load(&bytes)?;
    println!(
        "{} ({:?}, {} instances)",
        file.display(),
        loaded.format,
        loaded.dom.descendants().count().saturating_sub(1)
    );
    for &child in loaded.dom.root().children() {
        print_instance(&loaded.dom, child, 0, properties, depth);
    }
    Ok(())
}

fn print_instance(
    dom: &WeakDom,
    referent: rbx_types::Ref,
    level: usize,
    properties: bool,
    depth: Option<usize>,
) {
    let Some(inst) = dom.get_by_ref(referent) else {
        return;
    };
    let indent = "  ".repeat(level);
    println!("{indent}{} [{}]", inst.name, inst.class);
    if properties {
        print_properties(inst, &indent);
    }
    if depth.is_some_and(|d| level + 1 >= d) {
        if !inst.children().is_empty() {
            println!("{indent}  ... {} children", inst.children().len());
        }
        return;
    }
    for &child in inst.children() {
        print_instance(dom, child, level + 1, properties, depth);
    }
}

fn print_properties(inst: &Instance, indent: &str) {
    let mut props: Vec<_> = inst.properties.iter().collect();
    props.sort_unstable_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    for (name, value) in props {
        let mut text = format!("{value:?}");
        if text.len() > 120 {
            let cut = (0..=117)
                .rev()
                .find(|i| text.is_char_boundary(*i))
                .unwrap_or(0);
            text.truncate(cut);
            text.push_str("...");
        }
        println!("{indent}  . {name} = {text}");
    }
}

//! StyleRule selectors: parsing and matching instances.
//!
//! A selector is a comma-separated list of chains of compounds joined by
//! ` > ` (child) or ` >> ` (descendant); a compound names a class, `#Name`,
//! `.tag`s and `:State`s; a chain may end in `::Class`, a pseudo-instance
//! the rule creates under what it matches (creator-docs "Styling").
//! `@` queries aren't supported: a chain using one matches nothing.

use rook_dom::{Dom, InstanceId};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Compound {
    pub(crate) class: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) states: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Combinator {
    Child,
    Descendant,
}

/// One chain: compounds left to right, each with how it joins the one
/// before, and the pseudo-instance's class if it ends in one.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Chain {
    pub(crate) parts: Vec<(Combinator, Compound)>,
    pub(crate) pseudo: Option<String>,
}

#[cfg(test)]
impl Chain {
    /// The compound the matched instance itself must meet.
    fn subject(&self) -> &Compound {
        &self.parts.last().expect("chains aren't empty").1
    }
}

/// Parse a selector list; chains Rook can't follow are left out.
pub(crate) fn parse(selector: &str) -> Vec<Chain> {
    selector
        .split(',')
        .filter_map(|c| chain(c.trim()))
        .collect()
}

fn chain(text: &str) -> Option<Chain> {
    let (body, pseudo) = match text.split_once("::") {
        Some((body, pseudo)) => {
            let class = pseudo.trim();
            // `::StyleQuery #Name` and the like are queries.
            if class.is_empty() || class.contains(char::is_whitespace) {
                return None;
            }
            (body, Some(class.to_owned()))
        }
        None => (text, None),
    };
    let mut parts = Vec::new();
    let mut join = Combinator::Descendant;
    for word in body.split_whitespace() {
        match word {
            ">>" => join = Combinator::Descendant,
            ">" => join = Combinator::Child,
            _ => {
                parts.push((join, compound(word)?));
                join = Combinator::Descendant;
            }
        }
    }
    if parts.is_empty() {
        // `::UICorner` alone styles a pseudo-instance of everything.
        parts.push((Combinator::Descendant, Compound::default()));
    }
    Some(Chain { parts, pseudo })
}

/// The name at the start of `s`, and what follows it.
fn take(s: &str) -> (String, &str) {
    let end = s.find(['.', '#', ':', '@']).unwrap_or(s.len());
    (s[..end].to_owned(), &s[end..])
}

fn compound(word: &str) -> Option<Compound> {
    let mut c = Compound::default();
    let mut rest = word;
    if !rest.starts_with(['.', '#', ':', '@']) {
        let (class, after) = take(rest);
        c.class = Some(class);
        rest = after;
    }
    while let Some(kind) = rest.chars().next() {
        let (value, after) = take(&rest[1..]);
        match kind {
            '.' => c.tags.push(value),
            '#' => c.name = Some(value),
            ':' => c.states.push(value),
            _ => return None,
        }
        rest = after;
    }
    Some(c)
}

pub(crate) fn matches_compound(dom: &Dom, id: InstanceId, c: &Compound) -> bool {
    if let Some(class) = &c.class
        && !dom.is_a(id, class)
    {
        return false;
    }
    if let Some(name) = &c.name
        && dom.name(id) != name
    {
        return false;
    }
    c.tags.iter().all(|t| dom.has_tag(id, t))
        && c.states.iter().all(|s| {
            // Inference: states are the GuiState an object is in.
            let state = dom.enum_item_property(id, "GuiState").unwrap_or("Idle");
            match s.as_str() {
                "Hover" => state == "Hover",
                "Press" => state == "Press",
                "NonInteractable" => state == "NonInteractable",
                _ => false,
            }
        })
}

/// Whether `id`, inside `scope` (the linked tree's root), meets `chain`.
pub(crate) fn matches(dom: &Dom, scope: InstanceId, id: InstanceId, chain: &Chain) -> bool {
    let parts = &chain.parts;
    if !matches_compound(dom, id, &parts[parts.len() - 1].1) {
        return false;
    }
    let mut at = id;
    for i in (0..parts.len() - 1).rev() {
        let join = parts[i + 1].0;
        let wanted = &parts[i].1;
        let mut found = None;
        let mut up = at;
        while up != scope {
            let Some(parent) = dom.parent(up) else {
                break;
            };
            if matches_compound(dom, parent, wanted) {
                found = Some(parent);
                break;
            }
            if join == Combinator::Child {
                break;
            }
            up = parent;
        }
        match found {
            Some(f) => at = f,
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chains_tags_and_pseudo_instances() {
        let list = parse(".size-full-full, Frame#Bar.bg > TextLabel:Hover::UIStroke, @Query >> .x");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].subject().tags, ["size-full-full"]);
        let c = &list[1];
        assert_eq!(c.pseudo.as_deref(), Some("UIStroke"));
        assert_eq!(c.parts[0].1.class.as_deref(), Some("Frame"));
        assert_eq!(c.parts[0].1.name.as_deref(), Some("Bar"));
        assert_eq!(c.parts[1].0, Combinator::Child);
        assert_eq!(c.subject().states, ["Hover"]);
        assert!(parse(".x::StyleQuery #Wide").is_empty());
    }
}

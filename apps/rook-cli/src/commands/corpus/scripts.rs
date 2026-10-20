//! Static API-usage analysis of script sources.
//!
//! Walks every call chain and indexing chain (`a.b.c:d(...)`) and records:
//! method names, global function calls, `Library.function` calls, the class
//! names given to `Instance.new`, the services given to `GetService`, and
//! direct `game.X` accesses. Scripts are parsed as Luau, which accepts the
//! Lua 5.1 code old places contain.

use std::collections::BTreeMap;

use full_moon::ast::{
    Call, Expression, FunctionArgs, FunctionCall, Index, Prefix, Suffix, VarExpression,
};
use full_moon::tokenizer::{TokenReference, TokenType};
use full_moon::visitors::Visitor;
use full_moon::{LuaVersion, parse_fallible};

/// Counts of each kind of API use in one script.
#[derive(Debug, Default)]
pub(super) struct ScriptUsage {
    pub methods: BTreeMap<String, u64>,
    pub global_calls: BTreeMap<String, u64>,
    pub library_calls: BTreeMap<String, u64>,
    pub instance_new: BTreeMap<String, u64>,
    pub get_service: BTreeMap<String, u64>,
    pub game_index: BTreeMap<String, u64>,
}

/// Result of analysing one script.
#[derive(Debug)]
pub(super) struct ScriptAnalysis {
    pub usage: ScriptUsage,
    /// Number of syntax errors; the usage above covers whatever did parse.
    pub parse_errors: usize,
}

pub(super) fn analyse(source: &str) -> ScriptAnalysis {
    let result = parse_fallible(source, LuaVersion::luau());
    let mut visitor = UsageVisitor::default();
    visitor.visit_ast(result.ast());
    ScriptAnalysis {
        usage: visitor.usage,
        parse_errors: result.errors().len(),
    }
}

#[derive(Default)]
struct UsageVisitor {
    usage: ScriptUsage,
}

impl Visitor for UsageVisitor {
    fn visit_function_call(&mut self, node: &FunctionCall) {
        let suffixes: Vec<&Suffix> = node.suffixes().collect();
        self.chain(node.prefix(), &suffixes);
    }

    fn visit_var_expression(&mut self, node: &VarExpression) {
        let suffixes: Vec<&Suffix> = node.suffixes().collect();
        self.chain(node.prefix(), &suffixes);
    }
}

impl UsageVisitor {
    fn chain(&mut self, prefix: &Prefix, suffixes: &[&Suffix]) {
        let root = match prefix {
            Prefix::Name(token) => identifier(token),
            _ => None,
        };

        // `game.X` (the first index off `game`).
        if root.as_deref() == Some("game")
            && let Some(Suffix::Index(Index::Dot { name, .. })) = suffixes.first()
            && let Some(field) = identifier(name)
        {
            bump(&mut self.usage.game_index, format!("game.{field}"));
        }

        for (i, suffix) in suffixes.iter().enumerate() {
            match suffix {
                Suffix::Call(Call::MethodCall(method)) => {
                    let Some(name) = identifier(method.name()) else {
                        continue;
                    };
                    if name.eq_ignore_ascii_case("GetService")
                        && let Some(service) = first_string_arg(method.args())
                    {
                        bump(&mut self.usage.get_service, service);
                    }
                    bump(&mut self.usage.methods, name);
                }
                Suffix::Call(Call::AnonymousCall(args)) => match (i, root.as_deref()) {
                    // `name(...)`
                    (0, Some(name)) => bump(&mut self.usage.global_calls, name.to_owned()),
                    // `Lib.func(...)`
                    (1, Some(lib)) => {
                        if let Suffix::Index(Index::Dot { name, .. }) = suffixes[0]
                            && let Some(func) = identifier(name)
                        {
                            if lib == "Instance"
                                && func == "new"
                                && let Some(class) = first_string_arg(args)
                            {
                                bump(&mut self.usage.instance_new, class);
                            }
                            bump(&mut self.usage.library_calls, format!("{lib}.{func}"));
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

fn bump(map: &mut BTreeMap<String, u64>, key: String) {
    *map.entry(key).or_insert(0) += 1;
}

fn identifier(token: &TokenReference) -> Option<String> {
    match token.token_type() {
        TokenType::Identifier { identifier } => Some(identifier.to_string()),
        _ => None,
    }
}

fn string_literal(token: &TokenReference) -> Option<String> {
    match token.token_type() {
        TokenType::StringLiteral { literal, .. } => Some(literal.to_string()),
        _ => None,
    }
}

/// The first argument, if it's a string literal: `f("x")`, `f "x"`.
fn first_string_arg(args: &FunctionArgs) -> Option<String> {
    match args {
        FunctionArgs::String(token) => string_literal(token),
        FunctionArgs::Parentheses { arguments, .. } => match arguments.iter().next()? {
            Expression::String(token) => string_literal(token),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_calls_and_services() {
        let src = r#"
            local p = Instance.new("Part", workspace)
            local players = game:GetService("Players")
            game.Workspace.Brick.Touched:connect(function(hit)
                wait(1)
                hit.Parent:BreakJoints()
                print(math.random(1, 10))
            end)
            local lighting = game.Lighting
            spawn(function() end)
        "#;
        let a = analyse(src);
        assert_eq!(a.parse_errors, 0);
        let u = &a.usage;
        assert_eq!(u.instance_new.get("Part"), Some(&1));
        assert_eq!(u.get_service.get("Players"), Some(&1));
        assert_eq!(u.methods.get("connect"), Some(&1));
        assert_eq!(u.methods.get("BreakJoints"), Some(&1));
        assert_eq!(u.global_calls.get("wait"), Some(&1));
        assert_eq!(u.global_calls.get("spawn"), Some(&1));
        assert_eq!(u.library_calls.get("math.random"), Some(&1));
        assert_eq!(u.game_index.get("game.Workspace"), Some(&1));
        assert_eq!(u.game_index.get("game.Lighting"), Some(&1));
    }

    #[test]
    fn broken_scripts_still_report() {
        let a = analyse("wait(1)\nif then end end");
        assert!(a.parse_errors > 0);
    }
}

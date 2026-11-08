//! Every `fn` with a body, with the type or trait it belongs to.

use crate::lex::{find_match, Kind, Token};

#[derive(Clone, Debug)]
pub struct Func {
    pub name: String,
    /// `Type` for a method in `impl Type` / `impl Trait for Type`, `Trait` inside `trait Trait`.
    pub owner: Option<String>,
    pub file: String,
    pub line: u32,
    pub end_line: u32,
    /// The body's tokens, braces excluded.
    pub body: Vec<Token>,
}

impl Func {
    pub fn qualified(&self) -> String {
        match &self.owner {
            Some(o) => format!("{o}::{}", self.name),
            None => self.name.clone(),
        }
    }
}

/// Shape of a token stream: names -> `ID`, numbers -> `NUM`, literals -> `LIT`,
/// lifetimes -> `LT`; keywords and punctuation as they are.
pub fn shape(tokens: &[Token]) -> Vec<&str> {
    tokens
        .iter()
        .map(|t| match t.kind {
            Kind::Id => "ID",
            Kind::Num => "NUM",
            Kind::Lit => "LIT",
            Kind::Lifetime => "LT",
            Kind::Keyword | Kind::Punct => t.v.as_str(),
        })
        .collect()
}

/// `impl` or `trait` that starts an item (not `impl Trait` in a type).
fn starts_item(tokens: &[Token], i: usize) -> bool {
    match i.checked_sub(1).map(|p| &tokens[p]) {
        None => true,
        Some(p) => {
            ["}", ";", "{", "]"].iter().any(|s| p.is(s))
                || ["unsafe", "pub"].iter().any(|s| p.is(s))
                || (p.kind == Kind::Id && p.v == "default")
                || p.is(")") // pub(crate) trait
        }
    }
}

/// Index of the first `{` or `;` outside brackets and parentheses, from `from`.
fn header_end(tokens: &[Token], from: usize) -> usize {
    let mut depth = 0i32;
    for (k, t) in tokens.iter().enumerate().skip(from) {
        if t.kind != Kind::Punct {
            continue;
        }
        match t.v.as_str() {
            "(" | "[" => depth += 1,
            ")" | "]" => depth -= 1,
            "{" | ";" if depth == 0 => return k,
            _ => {}
        }
    }
    tokens.len()
}

/// The type an `impl` header is for: `impl<T> Trait for a::Foo<T> where ..` -> `Foo`.
fn impl_label(header: &[Token]) -> Option<String> {
    let mut h = header;
    if h.first().is_some_and(|t| t.is("<")) {
        let mut depth = 0;
        let mut cut = h.len();
        for (k, t) in h.iter().enumerate() {
            if t.is("<") {
                depth += 1;
            } else if t.is(">") {
                depth -= 1;
                if depth == 0 {
                    cut = k + 1;
                    break;
                }
            }
        }
        h = &h[cut.min(h.len())..];
    }
    let mut depth = 0;
    let mut start = 0;
    let mut end = h.len();
    for (k, t) in h.iter().enumerate() {
        if t.is("<") {
            depth += 1;
        } else if t.is(">") {
            depth -= 1;
        } else if depth == 0 && t.is("for") {
            start = k + 1;
        } else if depth == 0 && t.is("where") {
            end = k;
            break;
        }
    }
    let mut label = None;
    let mut depth = 0;
    for t in &h[start.min(end)..end] {
        if t.is("<") {
            depth += 1;
        } else if t.is(">") {
            depth -= 1;
        } else if depth == 0 && t.kind == Kind::Id {
            label = Some(t.v.clone());
        } else if depth == 0 && (t.is("(") || t.is("[")) && label.is_some() {
            break;
        }
    }
    label
}

/// All functions with a body, nested ones included, in source order.
pub fn extract_functions(tokens: &[Token], file: &str) -> Vec<Func> {
    let mut out = Vec::new();
    // (brace depth inside the block, owner label)
    let mut owners: Vec<(usize, String)> = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate() {
        if t.is("{") {
            depth += 1;
            if let Some((at, label)) = pending.take() {
                if at == i {
                    owners.push((depth, label));
                } else {
                    pending = Some((at, label));
                }
            }
            continue;
        }
        if t.is("}") {
            if owners.last().is_some_and(|(d, _)| *d == depth) {
                owners.pop();
            }
            depth = depth.saturating_sub(1);
            continue;
        }
        if (t.is("impl") || t.is("trait")) && starts_item(tokens, i) {
            let end = header_end(tokens, i + 1);
            if end < tokens.len() && tokens[end].is("{") {
                let label = if t.is("trait") {
                    tokens
                        .get(i + 1)
                        .filter(|x| x.kind == Kind::Id)
                        .map(|x| x.v.clone())
                } else {
                    impl_label(&tokens[i + 1..end])
                };
                if let Some(label) = label {
                    pending = Some((end, label));
                }
            }
            continue;
        }
        if t.is("fn") {
            let Some(name) = tokens.get(i + 1).filter(|x| x.kind == Kind::Id) else {
                continue;
            };
            let open = header_end(tokens, i + 2);
            if open >= tokens.len() || !tokens[open].is("{") {
                continue; // a declaration without a body
            }
            let close = find_match(tokens, open);
            out.push(Func {
                name: name.v.clone(),
                owner: owners.last().map(|(_, o)| o.clone()),
                file: file.to_string(),
                line: t.line,
                end_line: tokens[close].line,
                body: tokens[open + 1..close].to_vec(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::{code_tokens, tokenize};

    fn funcs(src: &str) -> Vec<(String, Option<String>, usize)> {
        let toks = code_tokens(tokenize(src), false);
        extract_functions(&toks, "f.rs")
            .into_iter()
            .map(|f| (f.name, f.owner, f.body.len()))
            .collect()
    }

    #[test]
    fn finds_free_fns_methods_and_nested() {
        let src = "fn a() -> u32 { fn inner(x: [u8; 2]) {} 1 }\n\
                   impl<T: Clone> Display for crate::Foo<T> where T: Copy { fn fmt(&self) -> String { s() } }\n\
                   trait Shape { fn area(&self) -> f64; fn twice(&self) -> f64 { 2.0 * self.area() } }\n\
                   pub(crate) fn g(f: fn(u8) -> u8, i: impl Fn()) {}";
        let f = funcs(src);
        let names: Vec<String> = f.iter().map(|x| x.0.clone()).collect();
        assert_eq!(names, ["a", "inner", "fmt", "twice", "g"]);
        assert_eq!(f[0].1, None);
        assert_eq!(f[2].1.as_deref(), Some("Foo"));
        assert_eq!(f[3].1.as_deref(), Some("Shape"));
        assert_eq!(f[4].1, None, "owner ends with the impl block");
    }

    #[test]
    fn shapes_erase_names_and_numbers() {
        let toks = tokenize("let x = a.b(3, \"s\");");
        assert_eq!(shape(&toks).join(" "), "let ID = ID . ID ( NUM , LIT ) ;");
    }
}

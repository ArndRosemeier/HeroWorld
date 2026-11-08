//! A small Rust tokenizer: enough to find functions and compare shapes, not a parser.
//!
//! Comments (nested block comments too) are dropped, every string, char and number
//! literal becomes one token, so text inside a literal can never fake code.
//! [`code_tokens`] then removes what is not logic: attributes, `use` items and,
//! unless asked, test code (`#[test]`, `#[cfg(test)]` items).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Id,
    Keyword,
    Num,
    Lit,
    Lifetime,
    Punct,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub v: String,
    pub kind: Kind,
    pub line: u32,
}

impl Token {
    pub fn is(&self, s: &str) -> bool {
        self.v == s && matches!(self.kind, Kind::Punct | Kind::Keyword)
    }
}

pub const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

const PUNCT3: &[&str] = &["..=", "..."];
const PUNCT2: &[&str] = &[
    "::", "->", "=>", "..", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "^=",
    "&=", "|=",
];

fn ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn ident_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Tokenize Rust source. Never fails: unterminated literals run to the end of the file.
pub fn tokenize(src: &str) -> Vec<Token> {
    let s: Vec<char> = src.chars().collect();
    let n = s.len();
    let at = |i: usize| if i < n { s[i] } else { '\0' };
    let mut out = Vec::new();
    let mut line = 1u32;
    let mut i = 0;
    while i < n {
        let c = s[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let start_line = line;
        let mut push = |v: String, kind: Kind| {
            out.push(Token {
                v,
                kind,
                line: start_line,
            })
        };
        if c == '/' && at(i + 1) == '/' {
            while i < n && s[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && at(i + 1) == '*' {
            let mut depth = 1;
            i += 2;
            while i < n && depth > 0 {
                if s[i] == '\n' {
                    line += 1;
                }
                if s[i] == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if s[i] == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        if c == '"' {
            i = skip_quoted(&s, i + 1, &mut line);
            push("LIT".into(), Kind::Lit);
            continue;
        }
        if c == '\'' {
            if at(i + 1) == '\\' {
                // Escaped char literal: '\n', '\'', '\u{..}'.
                let mut j = i + 3;
                while j < n && s[j] != '\'' && s[j] != '\n' {
                    j += 1;
                }
                i = j + 1;
                push("LIT".into(), Kind::Lit);
            } else if at(i + 2) == '\'' {
                i += 3;
                push("LIT".into(), Kind::Lit);
            } else {
                let mut j = i + 1;
                while j < n && ident_char(s[j]) {
                    j += 1;
                }
                push(s[i..j].iter().collect(), Kind::Lifetime);
                i = j;
            }
            continue;
        }
        if ident_start(c) {
            let mut j = i + 1;
            while j < n && ident_char(s[j]) {
                j += 1;
            }
            let word: String = s[i..j].iter().collect();
            let next = at(j);
            // Prefixed literals: b"..", c"..", b'x', r"..", r#".."#, br#".."#, cr"..".
            if matches!(word.as_str(), "b" | "c") && next == '"' {
                i = skip_quoted(&s, j + 1, &mut line);
                push("LIT".into(), Kind::Lit);
                continue;
            }
            if word == "b" && next == '\'' {
                let mut k = j + 1;
                if at(k) == '\\' {
                    k += 1;
                }
                k += 1;
                while k < n && s[k] != '\'' && s[k] != '\n' {
                    k += 1;
                }
                i = k + 1;
                push("LIT".into(), Kind::Lit);
                continue;
            }
            if matches!(word.as_str(), "r" | "br" | "cr") && (next == '"' || next == '#') {
                let mut hashes = 0;
                while at(j + hashes) == '#' {
                    hashes += 1;
                }
                if at(j + hashes) == '"' {
                    let mut k = j + hashes + 1;
                    loop {
                        if k >= n {
                            break;
                        }
                        if s[k] == '\n' {
                            line += 1;
                        }
                        if s[k] == '"' && (1..=hashes).all(|h| at(k + h) == '#') {
                            k += 1 + hashes;
                            break;
                        }
                        k += 1;
                    }
                    i = k;
                    push("LIT".into(), Kind::Lit);
                    continue;
                }
                if word == "r" && hashes == 1 && ident_start(at(j + 1)) {
                    // Raw identifier r#name.
                    let mut k = j + 2;
                    while k < n && ident_char(s[k]) {
                        k += 1;
                    }
                    push(s[j + 1..k].iter().collect(), Kind::Id);
                    i = k;
                    continue;
                }
            }
            let kind = if KEYWORDS.contains(&word.as_str()) {
                Kind::Keyword
            } else {
                Kind::Id
            };
            push(word, kind);
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i + 1;
            loop {
                let d = at(j);
                let fraction = d == '.' && at(j + 1).is_ascii_digit();
                let exponent_sign = (d == '+' || d == '-')
                    && matches!(s[j - 1], 'e' | 'E')
                    && !matches!(at(i + 1), 'x' | 'X')
                    && at(j + 1).is_ascii_digit();
                if !(ident_char(d) || fraction || exponent_sign) {
                    break;
                }
                j += 1;
            }
            // `1.` (a float with no fraction) but not `1..` or `1.max()`.
            if at(j) == '.' && at(j + 1) != '.' && !ident_start(at(j + 1)) {
                j += 1;
            }
            push(s[i..j].iter().collect(), Kind::Num);
            i = j;
            continue;
        }
        let rest = |len: usize| -> String { s[i..(i + len).min(n)].iter().collect() };
        if let Some(p) = PUNCT3.iter().find(|p| rest(3) == **p) {
            push((*p).to_string(), Kind::Punct);
            i += 3;
        } else if let Some(p) = PUNCT2.iter().find(|p| rest(2) == **p) {
            push((*p).to_string(), Kind::Punct);
            i += 2;
        } else {
            push(c.to_string(), Kind::Punct);
            i += 1;
        }
    }
    out
}

/// Index just past the closing quote of a `"..."` literal whose body starts at `i`.
fn skip_quoted(s: &[char], mut i: usize, line: &mut u32) -> usize {
    while i < s.len() {
        match s[i] {
            '\\' => {
                if s.get(i + 1) == Some(&'\n') {
                    *line += 1;
                }
                i += 2;
            }
            '"' => return i + 1,
            '\n' => {
                *line += 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    s.len()
}

/// Index of the token closing the bracket opened at `open`, or the last token.
pub fn find_match(tokens: &[Token], open: usize) -> usize {
    let mut depth = 0i32;
    for (k, t) in tokens.iter().enumerate().skip(open) {
        if t.kind != Kind::Punct {
            continue;
        }
        match t.v.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    tokens.len().saturating_sub(1)
}

/// End (inclusive) of the item starting at `from`: its `;` at depth 0, or
/// (unless `semicolon_only`, for `use a::{b, c};`) the `}` closing its block.
fn item_end(tokens: &[Token], from: usize, semicolon_only: bool) -> usize {
    let mut depth = 0i32;
    for (k, t) in tokens.iter().enumerate().skip(from) {
        if t.kind != Kind::Punct {
            continue;
        }
        match t.v.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" => depth -= 1,
            "}" => {
                depth -= 1;
                if depth <= 0 && !semicolon_only {
                    return k;
                }
            }
            ";" if depth == 0 => return k,
            _ => {}
        }
    }
    tokens.len().saturating_sub(1)
}

/// `#[test]`, `#[cfg(test)]`, `#[cfg(all(test, ..))]`, `#[tokio::test]`; never `cfg(not(test))`.
fn is_test_attr(inner: &[Token]) -> bool {
    let has = |w: &str| inner.iter().any(|t| t.kind == Kind::Id && t.v == w);
    match inner.first() {
        Some(t) if t.v == "cfg" => has("test") && !has("not"),
        Some(_) => inner.last().is_some_and(|t| t.v == "test") && !has("cfg_attr"),
        None => false,
    }
}

/// The tokens that are logic: attributes, `use` items and (unless `keep_tests`) test items removed.
pub fn code_tokens(tokens: Vec<Token>, keep_tests: bool) -> Vec<Token> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    let n = tokens.len();
    while i < n {
        let t = &tokens[i];
        if t.is("#") {
            let open = if tokens.get(i + 1).is_some_and(|x| x.is("!")) {
                i + 2
            } else {
                i + 1
            };
            if tokens.get(open).is_some_and(|x| x.is("[")) {
                let close = find_match(&tokens, open);
                let test = is_test_attr(&tokens[open + 1..close]);
                i = close + 1;
                if test && !keep_tests {
                    // Skip further attributes, then the whole item.
                    while tokens.get(i).is_some_and(|x| x.is("#"))
                        && tokens.get(i + 1).is_some_and(|x| x.is("["))
                    {
                        i = find_match(&tokens, i + 1) + 1;
                    }
                    if i < n {
                        i = item_end(&tokens, i, false) + 1;
                    }
                }
                continue;
            }
        }
        if t.is("use") || (t.is("extern") && tokens.get(i + 1).is_some_and(|x| x.is("crate"))) {
            i = item_end(&tokens, i, true) + 1;
            continue;
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(src: &str) -> Vec<String> {
        tokenize(src).into_iter().map(|t| t.v).collect()
    }

    #[test]
    fn literals_and_comments_never_make_code() {
        let v = vals(
            r####"let s = "fn fake() {}"; // fn b() {}
/* fn c() { /* nested */ } */ let r = r#"fn d() {}"#; let c = '}'; let b = b"x";"####,
        );
        assert!(!v.contains(&"fake".to_string()));
        assert!(!v.contains(&"d".to_string()));
        assert_eq!(v.iter().filter(|x| *x == "LIT").count(), 4);
        assert!(!v.contains(&"}".to_string()));
    }

    #[test]
    fn lifetimes_ranges_and_floats() {
        assert_eq!(vals("&'a str"), ["&", "'a", "str"]);
        assert_eq!(vals("0..n"), ["0", "..", "n"]);
        assert_eq!(vals("1.5e-3 + 2."), ["1.5e-3", "+", "2."]);
        assert_eq!(
            vals("x.0.max(1.0)"),
            ["x", ".", "0", ".", "max", "(", "1.0", ")"]
        );
        assert_eq!(vals("'\\''"), ["LIT"]);
    }

    #[test]
    fn lines_count_through_multiline_literals() {
        let t = tokenize("let a = \"x\ny\";\nfoo");
        assert_eq!(t.last().unwrap().line, 3);
    }

    #[test]
    fn test_items_attributes_and_uses_are_removed() {
        let src = "use a::{b, c};\n#[derive(Clone)]\nstruct S;\nfn live() {}\n#[cfg(test)]\nmod tests { fn t() {} }\n#[test]\nfn one() {}\n#[cfg(not(test))]\nfn kept() {}";
        let v: Vec<String> = code_tokens(tokenize(src), false)
            .into_iter()
            .map(|t| t.v)
            .collect();
        assert_eq!(v.join(" "), "struct S ; fn live ( ) { } fn kept ( ) { }");
        let all: Vec<String> = code_tokens(tokenize(src), true)
            .into_iter()
            .map(|t| t.v)
            .collect();
        assert!(all.contains(&"one".to_string()));
    }
}

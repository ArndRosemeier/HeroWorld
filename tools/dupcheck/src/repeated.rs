//! Copies under any name, and copies with no name (Toolbox's `repeated-code`).
//!
//! 1. Body clones: each body's shape is cut into 5-token shingles, MinHashed
//!    (16 bands x 4 rows) and bucketed; pairs sharing a bucket get an exact Jaccard
//!    and pairs at or above the floor are clustered.
//! 2. Repeated idioms: every window of W tokens is hashed with the names that say
//!    what code does kept (`.field`, `Type`, `path::item`, `call(`, `macro!`) and
//!    local names erased. A window found in enough files is extended to its longest
//!    shared run and ranked by files x specificity (IDF of its kept names).

use crate::extract::{shape, Func};
use crate::lex::{Kind, Token};
use std::collections::{BTreeSet, HashMap};

pub const SHINGLE: usize = 5;
pub const BANDS: usize = 16;
pub const ROWS: usize = 4;
pub const MAX_BUCKET: usize = 60;

#[derive(Clone, Debug)]
pub struct Opts {
    pub window: usize,
    pub min_files: usize,
    pub min_info: f64,
    pub min_body: usize,
    pub clone_floor: f64,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            window: 16,
            min_files: 3,
            min_info: 6.0,
            min_body: 40,
            clone_floor: 0.7,
        }
    }
}

pub fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

pub fn shingles(body: &[Token]) -> BTreeSet<u64> {
    let sh = shape(body);
    sh.windows(SHINGLE).map(|w| fnv(&w.join(" "))).collect()
}

pub fn minhash(set: &BTreeSet<u64>) -> Vec<u64> {
    let mut sig = vec![u64::MAX; BANDS * ROWS];
    for &x in set {
        for (i, s) in sig.iter_mut().enumerate() {
            let h = mix64(x ^ (i as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
            *s = (*s).min(h);
        }
    }
    sig
}

fn jaccard(a: &BTreeSet<u64>, b: &BTreeSet<u64>) -> f64 {
    let (s, l) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let inter = s.iter().filter(|x| l.contains(x)).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

#[derive(Default, Debug)]
pub struct CloneStats {
    pub too_small: usize,
    pub signed: usize,
    pub buckets: usize,
    pub oversize: usize,
    pub candidates: usize,
    pub nested: usize,
    pub ruled_out_size: usize,
    pub verified: usize,
    pub accepted: usize,
}

#[derive(Debug)]
pub struct Cluster {
    /// Indexes into the function list, sorted by file and line.
    pub fns: Vec<usize>,
    pub names: Vec<String>,
    pub files: usize,
    pub size: usize,
    pub min_sim: f64,
}

impl Cluster {
    pub fn same_name(&self) -> bool {
        self.names.len() == 1
    }
}

pub fn find_body_clones(funcs: &[Func], o: &Opts) -> (Vec<Cluster>, CloneStats) {
    let mut st = CloneStats::default();
    let mut items: Vec<(usize, BTreeSet<u64>, Vec<u64>)> = Vec::new();
    for (i, f) in funcs.iter().enumerate() {
        if f.body.len() < o.min_body {
            st.too_small += 1;
            continue;
        }
        let set = shingles(&f.body);
        if set.is_empty() {
            st.too_small += 1;
            continue;
        }
        let sig = minhash(&set);
        items.push((i, set, sig));
    }
    st.signed = items.len();

    let mut buckets: HashMap<(usize, &[u64]), Vec<usize>> = HashMap::new();
    for (idx, (_, _, sig)) in items.iter().enumerate() {
        for b in 0..BANDS {
            buckets
                .entry((b, &sig[b * ROWS..(b + 1) * ROWS]))
                .or_default()
                .push(idx);
        }
    }
    let mut lists: Vec<&Vec<usize>> = buckets.values().filter(|l| l.len() >= 2).collect();
    lists.sort();
    let mut seen = std::collections::HashSet::new();
    let mut pairs = Vec::new();
    for list in lists {
        st.buckets += 1;
        if list.len() > MAX_BUCKET {
            st.oversize += 1;
            continue;
        }
        for (x, &a) in list.iter().enumerate() {
            for &b in &list[x + 1..] {
                if !seen.insert((a.min(b), a.max(b))) {
                    continue;
                }
                st.candidates += 1;
                let (fa, fb) = (&funcs[items[a].0], &funcs[items[b].0]);
                if fa.file == fb.file && fa.line <= fb.end_line && fb.line <= fa.end_line {
                    st.nested += 1;
                    continue;
                }
                let (sa, sb) = (items[a].1.len(), items[b].1.len());
                if (sa.min(sb) as f64 / sa.max(sb) as f64) < o.clone_floor {
                    st.ruled_out_size += 1;
                    continue;
                }
                st.verified += 1;
                let sim = jaccard(&items[a].1, &items[b].1);
                if sim >= o.clone_floor {
                    st.accepted += 1;
                    pairs.push((a, b, sim));
                }
            }
        }
    }

    let mut parent: Vec<usize> = (0..items.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for &(a, b, _) in &pairs {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut groups: HashMap<usize, (BTreeSet<usize>, f64)> = HashMap::new();
    for &(a, b, sim) in &pairs {
        let r = find(&mut parent, a);
        let g = groups.entry(r).or_insert((BTreeSet::new(), 1.0));
        g.0.insert(items[a].0);
        g.0.insert(items[b].0);
        g.1 = g.1.min(sim);
    }
    let mut clusters: Vec<Cluster> = groups
        .into_values()
        .map(|(members, min_sim)| {
            let fns: Vec<usize> = members.into_iter().collect(); // funcs are sorted by file, line
            let names: BTreeSet<String> = fns.iter().map(|&i| funcs[i].name.clone()).collect();
            let files: BTreeSet<&str> = fns.iter().map(|&i| funcs[i].file.as_str()).collect();
            let size = fns.iter().map(|&i| funcs[i].body.len()).sum::<usize>() / fns.len();
            Cluster {
                names: names.into_iter().collect(),
                files: files.len(),
                size,
                min_sim: (min_sim * 100.0).round() / 100.0,
                fns,
            }
        })
        .collect();
    clusters.sort_by(|x, y| {
        x.same_name()
            .cmp(&y.same_name())
            .then((y.files > 1).cmp(&(x.files > 1)))
            .then((y.fns.len() * y.size).cmp(&(x.fns.len() * x.size)))
            .then(x.fns.cmp(&y.fns))
    });
    (clusters, st)
}

/// Words that are the language itself: kept in the shape, never counted as names.
fn structural(t: &Token) -> bool {
    t.kind == Kind::Keyword
}

const PRIMITIVES: &[&str] = &[
    "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128",
    "usize", "bool", "char", "str",
];

/// The idiom view of a token stream: the normalised strings, and per token the
/// name it carries for the specificity score (or `None`).
pub fn idiom_shape(tokens: &[Token]) -> (Vec<String>, Vec<Option<String>>) {
    let mut norm = Vec::with_capacity(tokens.len());
    let mut names = Vec::with_capacity(tokens.len());
    for (i, t) in tokens.iter().enumerate() {
        let prev = i.checked_sub(1).map(|p| &tokens[p]);
        let next = tokens.get(i + 1);
        let (n, name) = match t.kind {
            Kind::Num => ("NUM".to_string(), None),
            Kind::Lit => ("LIT".to_string(), None),
            Kind::Lifetime => ("LT".to_string(), None),
            Kind::Punct => (t.v.clone(), None),
            Kind::Keyword => (t.v.clone(), None),
            Kind::Id => {
                let v = &t.v;
                if prev.is_some_and(|p| p.is(".")) {
                    (v.clone(), Some(format!(".{v}")))
                } else if prev.is_some_and(|p| p.is("::")) {
                    (v.clone(), Some(format!("::{v}")))
                } else if next.is_some_and(|x| x.is("!")) {
                    (v.clone(), Some(format!("{v}!")))
                } else if next.is_some_and(|x| x.is("::"))
                    || v.starts_with(|c: char| c.is_uppercase())
                    || PRIMITIVES.contains(&v.as_str())
                {
                    (v.clone(), Some(v.clone()))
                } else if next.is_some_and(|x| x.is("(")) {
                    (v.clone(), Some(format!("{v}(")))
                } else {
                    ("ID".to_string(), None)
                }
            }
        };
        debug_assert!(!(structural(t) && name.is_some()));
        norm.push(n);
        names.push(name);
    }
    (norm, names)
}

pub struct Source {
    pub file: String,
    pub text: String,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone)]
pub struct At {
    pub file: String,
    pub line: u32,
    pub end_line: u32,
}

#[derive(Debug)]
pub struct Idiom {
    pub tokens: usize,
    pub info: f64,
    pub names: Vec<String>,
    pub at: Vec<At>,
    pub excerpt: Vec<String>,
}

#[derive(Default, Debug)]
pub struct IdiomStats {
    pub tokens: usize,
    pub windows: usize,
    pub recurring: usize,
    pub absorbed: usize,
    pub low_info: usize,
    pub reported: usize,
}

struct Group {
    start: Vec<(usize, usize)>,
    len: usize,
    info: f64,
    kept: Vec<String>,
}

pub fn find_idioms(sources: &[Source], o: &Opts) -> (Vec<Idiom>, Vec<Idiom>, IdiomStats) {
    let w = o.window.max(1);
    let mut st = IdiomStats::default();
    let mut vocab: HashMap<String, u32> = HashMap::new();
    let mut docs: Vec<(Vec<u32>, Vec<Option<String>>)> = Vec::new();
    let mut df: HashMap<String, usize> = HashMap::new();
    for s in sources {
        let (norm, names) = idiom_shape(&s.tokens);
        let ids: Vec<u32> = norm
            .into_iter()
            .map(|x| {
                let next = vocab.len() as u32;
                *vocab.entry(x).or_insert(next)
            })
            .collect();
        let distinct: BTreeSet<&String> = names.iter().flatten().collect();
        for n in distinct {
            *df.entry(n.clone()).or_default() += 1;
        }
        st.tokens += ids.len();
        docs.push((ids, names));
    }
    let n_docs = docs.len().max(1) as f64;
    let idf = |n: &str| (n_docs / *df.get(n).unwrap_or(&1) as f64).log2();

    // Every window, first occurrence per file.
    let mut occ: HashMap<u64, Vec<(usize, usize)>> = HashMap::new();
    for (d, (ids, _)) in docs.iter().enumerate() {
        for (i, win) in ids.windows(w).enumerate() {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for &x in win {
                h = (h ^ u64::from(x)).wrapping_mul(0x0100_0000_01b3);
            }
            st.windows += 1;
            let list = occ.entry(mix64(h)).or_default();
            if list.last().is_none_or(|&(ld, _)| ld != d) {
                list.push((d, i));
            }
        }
    }
    let ids = |d: usize| &docs[d].0;
    let mut lists: Vec<Vec<(usize, usize)>> = occ
        .into_values()
        .filter(|l| l.len() >= o.min_files)
        .collect();
    lists.sort();

    let mut groups = Vec::new();
    for list in lists {
        let r = list[0];
        let members: Vec<(usize, usize)> = list
            .into_iter()
            .filter(|&(d, i)| ids(d)[i..i + w] == ids(r.0)[r.1..r.1 + w])
            .collect();
        if members.len() < o.min_files {
            continue;
        }
        st.recurring += 1;
        let agree = |off: isize| -> bool {
            let at = |(d, i): (usize, usize)| -> Option<u32> {
                let k = i as isize + off;
                if k < 0 {
                    None
                } else {
                    ids(d).get(k as usize).copied()
                }
            };
            match at(members[0]) {
                None => false,
                Some(v0) => members.iter().all(|&m| at(m) == Some(v0)),
            }
        };
        let mut left = 0usize;
        while agree(-(left as isize) - 1) {
            left += 1;
        }
        let mut right = 0usize;
        while agree((w + right) as isize) {
            right += 1;
        }
        let start: Vec<(usize, usize)> = members.iter().map(|&(d, i)| (d, i - left)).collect();
        let len = w + left + right;
        let (d0, i0) = start[0];
        let kept: BTreeSet<&String> = docs[d0].1[i0..i0 + len].iter().flatten().collect();
        let info: f64 = kept.iter().map(|n| idf(n)).sum();
        groups.push(Group {
            start,
            len,
            info: (info * 10.0).round() / 10.0,
            kept: kept.into_iter().cloned().collect(),
        });
    }
    let pos = |g: &Group| (sources[g.start[0].0].file.clone(), g.start[0].1);
    groups.sort_by(|x, y| {
        (y.start.len() as f64 * y.info)
            .total_cmp(&(x.start.len() as f64 * x.info))
            .then(y.len.cmp(&x.len))
            .then(pos(x).cmp(&pos(y)))
            .then(x.start.cmp(&y.start))
    });

    let mut covered: Vec<Vec<(usize, usize)>> = vec![Vec::new(); docs.len()];
    let mut idioms = Vec::new();
    let mut filtered = Vec::new();
    for g in groups {
        let dup = g
            .start
            .iter()
            .filter(|&&(d, i)| {
                covered[d]
                    .iter()
                    .any(|&(ci, cl)| i < ci + cl && ci < i + g.len)
            })
            .count();
        if dup * 2 >= g.start.len() {
            st.absorbed += 1;
            continue;
        }
        for &(d, i) in &g.start {
            covered[d].push((i, g.len));
        }
        let entry = describe(&g, sources);
        if g.info < o.min_info {
            st.low_info += 1;
            filtered.push(entry);
        } else {
            st.reported += 1;
            idioms.push(entry);
        }
    }
    (idioms, filtered, st)
}

fn describe(g: &Group, sources: &[Source]) -> Idiom {
    let mut at: Vec<At> = g
        .start
        .iter()
        .map(|&(d, i)| {
            let t = &sources[d].tokens;
            At {
                file: sources[d].file.clone(),
                line: t[i].line,
                end_line: t[i + g.len - 1].line,
            }
        })
        .collect();
    at.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    let (d, i) = g.start[0];
    let src = &sources[d];
    let (l0, l1) = (src.tokens[i].line, src.tokens[i + g.len - 1].line);
    let mut excerpt: Vec<String> = src
        .text
        .lines()
        .skip(l0 as usize - 1)
        .take((l1.min(l0 + 7) - l0 + 1) as usize)
        .map(|l| l.trim_end().to_string())
        .collect();
    if l1 > l0 + 7 {
        excerpt.push("…".into());
    }
    Idiom {
        tokens: g.len,
        info: g.info,
        names: g.kept.clone(),
        at,
        excerpt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract_functions;
    use crate::lex::{code_tokens, tokenize};

    #[test]
    fn idiom_shape_keeps_what_code_does() {
        let t = tokenize("let d = glam::DVec3::new(p.x, 0.0, q.z).length(); clamp(v)");
        let (norm, names) = idiom_shape(&t);
        assert_eq!(norm[1], "ID");
        let kept: Vec<String> = names.into_iter().flatten().collect();
        assert_eq!(
            kept,
            ["glam", "::DVec3", "::new", ".x", ".z", ".length", "clamp("]
        );
    }

    fn body(name: &str, k: f64) -> String {
        format!(
            "fn {name}(a: f64, b: f64) -> f64 {{ let mut s = 0.0; for i in 0..10 {{ s += a * {k} + b / (i as f64 + 1.0); if s > 100.0 {{ s -= 50.0; }} }} let t = s.sqrt().max(a).min(b); t * t + s }}"
        )
    }

    #[test]
    fn body_clones_under_different_names() {
        let mut funcs = Vec::new();
        for (file, src) in [
            ("a.rs", body("alpha", 2.0)),
            ("b.rs", body("beta", 3.0)),
            (
                "c.rs",
                "fn gamma() { let v = vec![1, 2, 3]; println!(\"{v:?}\"); }".to_string(),
            ),
        ] {
            funcs.extend(extract_functions(&code_tokens(tokenize(&src), false), file));
        }
        let (clusters, st) = find_body_clones(
            &funcs,
            &Opts {
                min_body: 10,
                ..Opts::default()
            },
        );
        assert_eq!(clusters.len(), 1, "{st:?}");
        assert_eq!(clusters[0].names, ["alpha", "beta"]);
        assert_eq!(clusters[0].files, 2);
    }

    #[test]
    fn idioms_recur_across_files() {
        let snippet = "let w = road.half_width() * 2.0 + kerb.height; let n = (w / lamp.spacing).ceil() as usize;";
        let sources: Vec<Source> = ["a", "b", "c", "d"]
            .iter()
            .enumerate()
            .map(|(k, f)| {
                let text = if k < 3 {
                    format!("fn f{k}(q: u8) {{ let z{k} = {k}; {snippet} other{k}(); }}")
                } else {
                    "fn lonely() { nothing_here(); }".to_string()
                };
                Source {
                    file: format!("{f}.rs"),
                    tokens: code_tokens(tokenize(&text), false),
                    text,
                }
            })
            .collect();
        let (idioms, _, _) = find_idioms(
            &sources,
            &Opts {
                min_info: 0.0,
                ..Opts::default()
            },
        );
        assert_eq!(idioms.len(), 1);
        assert_eq!(idioms[0].at.len(), 3);
        assert!(idioms[0].names.contains(&".spacing".to_string()));
    }
}

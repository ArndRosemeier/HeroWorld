//! Same-name functions across files (Toolbox's `duplicate-candidates`).
//!
//! Three tiers, each pair in the strongest one only: exact name, normalised name
//! (case and `_` ignored), near name (affixes stripped, same word set or edit
//! distance <= 2). A body-shape Jaccard is attached as a HINT, with prune-first
//! early exits whose counters are printed.

use crate::extract::{shape, Func};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub const SIM_FLOOR: f64 = 0.3;
pub const CONFIRM_FLOOR: f64 = 0.6;
pub const SIZE_RATIO: f64 = 0.5;
/// Edit distance only links names at least this long (affixes stripped):
/// Rust names are short, and `up` / `u16` or `cell` / `tell` are noise.
pub const NEAR_MIN_LEN: usize = 6;

/// Names that repeat by construction in Rust: trait methods, constructors, `main`.
/// A pair is moved out of the listing only when every name in it is here.
pub const DEFAULT_IGNORED: &[&str] = &[
    "new",
    "default",
    "fmt",
    "from",
    "try_from",
    "into",
    "clone",
    "eq",
    "ne",
    "partial_cmp",
    "cmp",
    "hash",
    "drop",
    "deref",
    "deref_mut",
    "as_ref",
    "as_mut",
    "borrow",
    "from_str",
    "from_iter",
    "into_iter",
    "next",
    "index",
    "index_mut",
    "add",
    "sub",
    "mul",
    "div",
    "neg",
    "add_assign",
    "sub_assign",
    "mul_assign",
    "main",
];

pub const AFFIXES: &[&str] = &[
    "get", "set", "handle", "on", "try", "do", "make", "create", "build", "update", "process",
    "render", "load", "save", "init", "is", "has", "can", "should",
];

pub fn normalise(name: &str) -> String {
    name.to_lowercase().replace('_', "")
}

/// `snake_case`, `CamelCase` and digit boundaries -> lowercase words.
pub fn split_words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut words = Vec::new();
    let mut cur = String::new();
    for (k, &c) in chars.iter().enumerate() {
        if c == '_' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if let Some(&p) = k.checked_sub(1).map(|p| &chars[p]) {
            let next_lower = chars.get(k + 1).is_some_and(|n| n.is_lowercase());
            let boundary = (p.is_lowercase() || p.is_ascii_digit()) && c.is_uppercase()
                || (p.is_uppercase() && c.is_uppercase() && next_lower)
                || (p.is_alphabetic() && c.is_ascii_digit())
                || (p.is_ascii_digit() && c.is_alphabetic());
            if boundary && !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
        }
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// Strip leading and trailing affix words; never returns an empty list.
pub fn strip_affixes(words: &[String]) -> Vec<String> {
    let mut lo = 0;
    let mut hi = words.len();
    while hi - lo > 1 && AFFIXES.contains(&words[lo].as_str()) {
        lo += 1;
    }
    while hi - lo > 1 && AFFIXES.contains(&words[hi - 1].as_str()) {
        hi -= 1;
    }
    words[lo..hi].to_vec()
}

/// Bounded Levenshtein distance (returns `max + 1` once it is exceeded).
pub fn levenshtein(a: &str, b: &str, max: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a == b {
        return 0;
    }
    if a.len().abs_diff(b.len()) > max {
        return max + 1;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i];
        let mut row_min = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let v = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            cur.push(v);
            row_min = row_min.min(v);
        }
        if row_min > max {
            return max + 1;
        }
        prev = cur;
    }
    prev[b.len()]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Compared,
    Confirmed,
    RuledOutSize,
    RuledOutDisjoint,
}

#[derive(Default, Debug)]
pub struct Stats {
    pub pairs: usize,
    pub compared: usize,
    pub confirmed: usize,
    pub pruned_size: usize,
    pub pruned_disjoint: usize,
    pub shape_sets: usize,
    pub examined: usize,
}

#[derive(Clone, Debug)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub tier: u8,
    pub status: Status,
    pub sim: Option<f64>,
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

struct Evidence<'a> {
    funcs: &'a [Func],
    sets: Vec<Option<BTreeSet<&'a str>>>,
    stats: Stats,
}

impl<'a> Evidence<'a> {
    fn set(&mut self, i: usize) -> &BTreeSet<&'a str> {
        if self.sets[i].is_none() {
            self.sets[i] = Some(shape(&self.funcs[i].body).into_iter().collect());
            self.stats.shape_sets += 1;
        }
        self.sets[i].as_ref().unwrap()
    }

    /// Prune-first body comparison; see the README for the four outcomes.
    fn compare(&mut self, a: usize, b: usize) -> (Status, Option<f64>) {
        let (sa, sb) = (self.funcs[a].body.len(), self.funcs[b].body.len());
        let (short, long) = (sa.min(sb), sa.max(sb));
        if long == 0 || (short as f64 / long as f64) < SIZE_RATIO {
            self.stats.pruned_size += 1;
            return (Status::RuledOutSize, None);
        }
        let set_a = self.set(a).clone();
        let set_b = self.set(b);
        let (small, large) = if set_a.len() <= set_b.len() {
            (&set_a, set_b)
        } else {
            (set_b, &set_a)
        };
        let union = (set_a.len() + set_b.len()) as f64;
        let mut inter = 0usize;
        let mut examined = 0usize;
        let mut result = None;
        for tok in small.iter() {
            examined += 1;
            if large.contains(tok) {
                inter += 1;
            }
            let remaining = small.len() - examined;
            let max_inter = (inter + remaining) as f64;
            let max_sim = if max_inter == 0.0 {
                0.0
            } else {
                max_inter / (union - max_inter)
            };
            if max_sim < SIM_FLOOR {
                result = Some((Status::RuledOutDisjoint, None));
                break;
            }
            let min_sim = if inter == 0 {
                0.0
            } else {
                inter as f64 / (union - inter as f64)
            };
            if remaining > 0 && min_sim >= CONFIRM_FLOOR {
                result = Some((Status::Confirmed, Some(round2(min_sim))));
                break;
            }
        }
        self.stats.examined += examined;
        let result = result.unwrap_or_else(|| {
            let sim = if inter == 0 {
                0.0
            } else {
                inter as f64 / (union - inter as f64)
            };
            (Status::Compared, Some(round2(sim)))
        });
        match result.0 {
            Status::Compared => self.stats.compared += 1,
            Status::Confirmed => self.stats.confirmed += 1,
            Status::RuledOutDisjoint => self.stats.pruned_disjoint += 1,
            Status::RuledOutSize => self.stats.pruned_size += 1,
        }
        result
    }
}

pub struct Result {
    pub tiers: [Vec<Pair>; 3],
    pub ignored: Vec<Pair>,
    pub ignored_by_name: BTreeMap<String, usize>,
    pub stats: Stats,
}

fn order_key(f: &Func) -> (&str, u32, &str) {
    (&f.file, f.line, &f.name)
}

pub fn pair_name(funcs: &[Func], p: &Pair) -> String {
    let (a, b) = (&funcs[p.a].name, &funcs[p.b].name);
    if a == b {
        a.clone()
    } else {
        format!("{a} / {b}")
    }
}

/// Group functions into the three tiers. `funcs` must be sorted by file, line, name.
pub fn group(funcs: &[Func], ignore: &[String]) -> Result {
    let ignore: HashSet<String> = ignore.iter().map(|n| normalise(n)).collect();
    let mut ev = Evidence {
        funcs,
        sets: vec![None; funcs.len()],
        stats: Stats::default(),
    };
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut ignored = Vec::new();
    let mut ignored_by_name: BTreeMap<String, usize> = BTreeMap::new();

    let cross = |list: &[usize]| -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        for (x, &i) in list.iter().enumerate() {
            for &j in &list[x + 1..] {
                if funcs[i].file != funcs[j].file {
                    let (a, b) = if order_key(&funcs[i]) <= order_key(&funcs[j]) {
                        (i, j)
                    } else {
                        (j, i)
                    };
                    v.push((a, b));
                }
            }
        }
        v
    };

    let mut process = |pairs: Vec<(usize, usize)>,
                       tier: u8,
                       ev: &mut Evidence,
                       ignored: &mut Vec<Pair>,
                       by_name: &mut BTreeMap<String, usize>|
     -> Vec<Pair> {
        let mut kept = Vec::new();
        for (a, b) in pairs {
            if !seen.insert((a, b)) {
                continue;
            }
            ev.stats.pairs += 1;
            let (status, sim) = ev.compare(a, b);
            let p = Pair {
                a,
                b,
                tier,
                status,
                sim,
            };
            let all_ignored = [&funcs[a].name, &funcs[b].name]
                .iter()
                .all(|n| ignore.contains(&normalise(n)));
            if all_ignored {
                *by_name.entry(pair_name(funcs, &p)).or_default() += 1;
                ignored.push(p);
            } else {
                kept.push(p);
            }
        }
        sort_pairs(funcs, &mut kept);
        kept
    };

    let group_by = |key: &dyn Fn(&Func) -> String| -> BTreeMap<String, Vec<usize>> {
        let mut m: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, f) in funcs.iter().enumerate() {
            m.entry(key(f)).or_default().push(i);
        }
        m
    };

    let exact: Vec<_> = group_by(&|f| f.name.clone())
        .values()
        .flat_map(|l| cross(l))
        .collect();
    let t1 = process(exact, 1, &mut ev, &mut ignored, &mut ignored_by_name);
    let normal: Vec<_> = group_by(&|f| normalise(&f.name))
        .values()
        .flat_map(|l| cross(l))
        .collect();
    let t2 = process(normal, 2, &mut ev, &mut ignored, &mut ignored_by_name);

    let by_name = group_by(&|f| f.name.clone());
    let prep: Vec<(&String, String, String)> = by_name
        .keys()
        .map(|n| {
            let words = strip_affixes(&split_words(n));
            let mut set: Vec<String> = words.clone();
            set.sort();
            set.dedup();
            (n, set.join("|"), words.concat())
        })
        .collect();
    let mut near = Vec::new();
    for (x, (na, set_a, joined_a)) in prep.iter().enumerate() {
        for (nb, set_b, joined_b) in &prep[x + 1..] {
            let close = set_a == set_b
                || (joined_a.len().min(joined_b.len()) >= NEAR_MIN_LEN
                    && joined_a.len().abs_diff(joined_b.len()) <= 2
                    && levenshtein(joined_a, joined_b, 2) <= 2);
            if close {
                let mut list = by_name[*na].clone();
                list.extend(&by_name[*nb]);
                list.sort_by(|&i, &j| order_key(&funcs[i]).cmp(&order_key(&funcs[j])));
                near.extend(cross(&list));
            }
        }
    }
    let t3 = process(near, 3, &mut ev, &mut ignored, &mut ignored_by_name);

    ignored.sort_by(|x, y| {
        pair_name(funcs, x)
            .cmp(&pair_name(funcs, y))
            .then(cmp_sim(y.sim, x.sim))
            .then(order_key(&funcs[x.a]).cmp(&order_key(&funcs[y.a])))
            .then(order_key(&funcs[x.b]).cmp(&order_key(&funcs[y.b])))
    });
    Result {
        tiers: [t1, t2, t3],
        ignored,
        ignored_by_name,
        stats: ev.stats,
    }
}

fn cmp_sim(a: Option<f64>, b: Option<f64>) -> std::cmp::Ordering {
    a.unwrap_or(0.0).total_cmp(&b.unwrap_or(0.0))
}

fn status_rank(s: Status) -> u8 {
    match s {
        Status::Compared | Status::Confirmed => 0,
        Status::RuledOutSize => 1,
        Status::RuledOutDisjoint => 2,
    }
}

fn sort_pairs(funcs: &[Func], pairs: &mut [Pair]) {
    pairs.sort_by(|x, y| {
        status_rank(x.status)
            .cmp(&status_rank(y.status))
            .then(cmp_sim(y.sim, x.sim))
            .then(funcs[x.a].name.cmp(&funcs[y.a].name))
            .then(order_key(&funcs[x.a]).cmp(&order_key(&funcs[y.a])))
            .then(order_key(&funcs[x.b]).cmp(&order_key(&funcs[y.b])))
    });
}

pub fn evidence_text(p: &Pair) -> String {
    match (p.status, p.sim) {
        (Status::Confirmed, Some(s)) => format!("sim>={s:.2} (confirmed early)"),
        (Status::Compared, Some(s)) => format!("sim={s:.2}"),
        (Status::RuledOutSize, _) => "ruled-out (size)".into(),
        _ => "ruled-out (disjoint)".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::{code_tokens, tokenize};

    #[test]
    fn name_helpers() {
        assert_eq!(
            split_words("getHTTPServer2_name"),
            ["get", "http", "server", "2", "name"]
        );
        assert_eq!(normalise("Add_Log_Entry"), "addlogentry");
        let w = strip_affixes(&split_words("build_road_mesh"));
        assert_eq!(w, ["road", "mesh"]);
        assert_eq!(strip_affixes(&split_words("update")), ["update"]);
        assert_eq!(levenshtein("kitten", "sitting", 5), 3);
        assert_eq!(levenshtein("abc", "abcdef", 2), 3);
    }

    fn fixture() -> Vec<Func> {
        let a = "fn road_mesh(w: f32) -> f32 { let x = w * 2.0; x + 1.0 }\nfn new() {}\nfn alone() { 1 }";
        let b = "fn road_mesh(v: f32) -> f32 { let y = v * 3.0; y + 4.0 }\nfn new() {}\nfn build_road_mesh(q: f32) { let z = q * 2.0; z + 1.0 }";
        let mut out = Vec::new();
        for (file, src) in [("a.rs", a), ("b.rs", b)] {
            out.extend(crate::extract::extract_functions(
                &code_tokens(tokenize(src), false),
                file,
            ));
        }
        out.sort_by(|x, y| order_key(x).cmp(&order_key(y)));
        out
    }

    #[test]
    fn tiers_and_ignore_list() {
        let f = fixture();
        let ignore: Vec<String> = DEFAULT_IGNORED.iter().map(|s| s.to_string()).collect();
        let r = group(&f, &ignore);
        assert_eq!(r.tiers[0].len(), 1);
        assert_eq!(pair_name(&f, &r.tiers[0][0]), "road_mesh");
        assert!(r.tiers[0][0].sim.is_some_and(|s| s >= CONFIRM_FLOOR));
        assert_eq!(
            r.tiers[2].len(),
            1,
            "build_road_mesh is near road_mesh in the other file"
        );
        assert_eq!(r.ignored.len(), 1);
        assert_eq!(r.ignored_by_name["new"], 1);
        let all = group(&f, &[]);
        assert_eq!(all.tiers[0].len(), 2);
    }
}

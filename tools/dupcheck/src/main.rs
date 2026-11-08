//! dupcheck: finds duplicated code in a Rust workspace. A pointer, not a judge.
//!
//! A Rust port of the two duplicate finders in Toolbox
//! (<https://github.com/ArndRosemeier/Toolbox>, `tools/duplicate-candidates` and
//! `tools/repeated-code`). It writes two reports and always exits 0 (2 on bad
//! arguments): deciding what a candidate means is a human call.
//!
//! ```text
//! cargo run --release -p dupcheck                  # scans crates/, writes reports/
//! cargo run --release -p dupcheck -- --help
//! ```

mod extract;
mod lex;
mod names;
mod repeated;

use extract::Func;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const HELP: &str = "dupcheck: duplicated-code CANDIDATES in Rust sources (needs human review)

Usage: cargo run --release -p dupcheck -- [options]

  --src DIR          source root, repeatable (default: crates)
  --report-dir DIR   where the two reports go (default: reports)
  --only names|repeated   run one of the two finders
  --tests            include test code (#[test], #[cfg(test)], tests/ and benches/)
  --max N            entries per section on stdout (default 25; the reports have all)
  --ignore a,b,c     replace the ubiquitous-name list of the name finder
  --no-ignore        list every same-name pair, trait methods included
  --window N         idiom window in tokens (default 16)
  --min-files N      an idiom must recur in N files (default 3)
  --min-info BITS    idiom specificity floor (default 6; 0 lists everything)
  --min-body N       smallest body in tokens for body clones (default 40)
  --clone-floor X    body Jaccard that counts as a clone (default 0.7)
  --generated VALUE  stamp the reports (omitted: runs are byte-identical)
";

struct Args {
    src: Vec<PathBuf>,
    report_dir: PathBuf,
    only: Option<String>,
    tests: bool,
    max: usize,
    ignore: Vec<String>,
    generated: Option<String>,
    opts: repeated::Opts,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn parse_args() -> Result<Args, String> {
    let root = repo_root();
    let mut a = Args {
        src: Vec::new(),
        report_dir: root.join("reports"),
        only: None,
        tests: false,
        max: 25,
        ignore: names::DEFAULT_IGNORED
            .iter()
            .map(|s| s.to_string())
            .collect(),
        generated: None,
        opts: repeated::Opts::default(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut val = || it.next().ok_or(format!("{flag} needs a value"));
        let num = |v: String| v.parse::<f64>().map_err(|_| format!("bad number: {v}"));
        match flag.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--src" => a.src.push(PathBuf::from(val()?)),
            "--report-dir" => a.report_dir = PathBuf::from(val()?),
            "--only" => {
                let v = val()?;
                if v != "names" && v != "repeated" {
                    return Err(format!("--only takes names or repeated, not {v}"));
                }
                a.only = Some(v);
            }
            "--tests" => a.tests = true,
            "--max" => a.max = num(val()?)? as usize,
            "--ignore" => {
                a.ignore = val()?
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "--no-ignore" => a.ignore.clear(),
            "--window" => a.opts.window = num(val()?)? as usize,
            "--min-files" => a.opts.min_files = num(val()?)? as usize,
            "--min-info" => a.opts.min_info = num(val()?)?,
            "--min-body" => a.opts.min_body = num(val()?)? as usize,
            "--clone-floor" => a.opts.clone_floor = num(val()?)?,
            "--generated" => a.generated = Some(val()?),
            other => return Err(format!("unknown option {other} (see --help)")),
        }
    }
    if a.src.is_empty() {
        a.src.push(root.join("crates"));
    }
    Ok(a)
}

/// `.rs` files under `dir`, sorted; skips `target`, hidden dirs, symlinks and,
/// unless `tests`, `tests/` and `benches/`.
fn list_files(dir: &Path, tests: bool, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let Ok(ft) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().to_string();
        if ft.is_symlink() || name.starts_with('.') {
            continue;
        }
        if ft.is_dir() {
            if name == "target" || (!tests && (name == "tests" || name == "benches")) {
                continue;
            }
            list_files(&e.path(), tests, out);
        } else if name.ends_with(".rs") {
            out.push(e.path());
        }
    }
}

fn display(path: &Path, root: &Path) -> String {
    let p = std::fs::canonicalize(path).unwrap_or(path.to_path_buf());
    let r = std::fs::canonicalize(root).unwrap_or(root.to_path_buf());
    p.strip_prefix(&r)
        .unwrap_or(&p)
        .to_string_lossy()
        .replace('\\', "/")
}

struct Scan {
    files: usize,
    skipped: Vec<String>,
    funcs: Vec<Func>,
    sources: Vec<repeated::Source>,
}

fn scan(a: &Args) -> Scan {
    let root = repo_root();
    let mut paths = Vec::new();
    for s in &a.src {
        list_files(s, a.tests, &mut paths);
    }
    let mut out = Scan {
        files: paths.len(),
        skipped: Vec::new(),
        funcs: Vec::new(),
        sources: Vec::new(),
    };
    for p in &paths {
        let file = display(p, &root);
        let text = match std::fs::read_to_string(p) {
            Ok(t) => t,
            Err(e) => {
                out.skipped.push(format!("{file}: {e}"));
                continue;
            }
        };
        let tokens = lex::code_tokens(lex::tokenize(&text), a.tests);
        out.funcs.extend(extract::extract_functions(&tokens, &file));
        out.sources.push(repeated::Source { file, text, tokens });
    }
    out.funcs
        .sort_by(|x, y| (&x.file, x.line, &x.name).cmp(&(&y.file, y.line, &y.name)));
    out
}

fn loc(f: &Func) -> String {
    let q = f.qualified();
    format!("{}:{} ({q})", f.file, f.line)
}

const NAMES_HEADER: &str = "Duplicate-function CANDIDATES (name-based discovery, NOT verdicts)
These are CANDIDATES that require HUMAN REVIEW. A shared name is a question:
if one changes, must the other change too? `sim` compares body shapes and is only a hint.";

const WHAT_TO_DO: &str = "## What to do with a candidate

Read both side by side and ask: if one changes, must the other change too?

- **No**: different jobs that share a name. Leave them, or rename one so the difference shows.
- **Yes, literally the same thing**: keep the better one (in the crate both can depend on), point every caller at it, delete the other. Look for near-invisible differences first (bounds, `<` vs `<=`, a different default); finding one moves you to the next way.
- **Yes, big and almost the same with one twist**: one function with one parameter for the twist, defaulting to today's behaviour; test both settings. For a short almost-duplicate, leaving it is usually cheaper.
- **A repeated idiom** is usually a missing helper: name it once, replace the copies.
- **Or leave it**, as a decision. Never merge or delete code only to make this list shorter.

Mind the crate rules in AGENTS.md: a shared helper goes where every user may depend on it (`hw-core` for maths and hashing), never into a crate that would pull Engine into generation or the server.
";

fn names_report(a: &Args, s: &Scan) -> (String, String) {
    let r = names::group(&s.funcs, &a.ignore);
    let st = &r.stats;
    let pct = |n: usize| 100.0 * n as f64 / st.pairs.max(1) as f64;
    let stats = vec![
        format!(
            "Scan: {} files, {} functions with a body, {} skipped{}",
            s.files,
            s.funcs.len(),
            s.skipped.len(),
            if a.tests { "" } else { " (test code left out; --tests includes it)" }
        ),
        format!(
            "Tiers: {} exact, {} normalised, {} near (each pair once, in its strongest tier)",
            r.tiers[0].len(),
            r.tiers[1].len(),
            r.tiers[2].len()
        ),
        format!(
            "Ignored as ubiquitous: {} pairs ({}){}",
            r.ignored.len(),
            r.ignored_by_name
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join(", "),
            if a.ignore.is_empty() { " [--no-ignore]" } else { "" }
        ),
        format!(
            "Evidence: {} comparisons; fully compared {} ({:.1}%), confirmed early {} ({:.1}%), ruled out by size {} ({:.1}%), ruled out as disjoint {} ({:.1}%); {} shape sets built, {} shape tokens examined",
            st.pairs, st.compared, pct(st.compared), st.confirmed, pct(st.confirmed),
            st.pruned_size, pct(st.pruned_size), st.pruned_disjoint, pct(st.pruned_disjoint),
            st.shape_sets, st.examined
        ),
    ];
    let line = |p: &names::Pair| {
        format!(
            "{}()  {}  {}  <->  {}",
            names::pair_name(&s.funcs, p),
            names::evidence_text(p),
            loc(&s.funcs[p.a]),
            loc(&s.funcs[p.b])
        )
    };
    let titles = [
        "TIER 1: exact name across files",
        "TIER 2: same name ignoring case and _",
        "TIER 3: near name (affixes stripped, same words or edit distance <= 2)",
    ];
    let mut out = format!("{NAMES_HEADER}\n\n");
    for l in &stats {
        let _ = writeln!(out, "{l}");
    }
    let mut md = String::from("# Duplicate-function candidates\n\n");
    for l in NAMES_HEADER.lines() {
        let _ = writeln!(md, "> {l}");
    }
    md.push('\n');
    if let Some(g) = &a.generated {
        let _ = writeln!(md, "- Generated: {g}\n");
    }
    md.push_str("## Scan stats\n\n");
    for l in &stats {
        let _ = writeln!(md, "- {l}");
    }
    for skipped in &s.skipped {
        let _ = writeln!(md, "- Skipped: {skipped}");
    }
    for (t, pairs) in r.tiers.iter().enumerate() {
        let _ = writeln!(out, "\n== {} ({} pairs) ==", titles[t], pairs.len());
        for p in pairs.iter().take(a.max) {
            let _ = writeln!(out, "  {}", line(p));
        }
        if pairs.len() > a.max {
            let _ = writeln!(out, "  ... {} more in the report", pairs.len() - a.max);
        }
        let _ = writeln!(md, "\n## {} ({} pairs)\n", titles[t], pairs.len());
        for p in pairs {
            let _ = writeln!(md, "- `{}`", line(p));
        }
    }
    let _ = writeln!(
        md,
        "\n## Ignored candidates: ubiquitous names ({} pairs)\n\nEvery name in these pairs is on the ignore list (`{}`). Listed in full; `--no-ignore` puts them back in the tiers.\n",
        r.ignored.len(),
        a.ignore.join(", ")
    );
    for p in &r.ignored {
        let _ = writeln!(md, "- `{}` (tier {})", line(p), p.tier);
    }
    let _ = write!(md, "\n{WHAT_TO_DO}");
    (out, md)
}

const REPEATED_HEADER: &str =
    "Repeated-code CANDIDATES (copies under any name, and inline copies; NOT verdicts)
These are CANDIDATES that require HUMAN REVIEW. Two snippets with the same shape
can do different jobs; a recurring idiom can be fine (or a missing helper).";

fn repeated_report(a: &Args, s: &Scan) -> (String, String) {
    let o = &a.opts;
    let (clusters, cs) = repeated::find_body_clones(&s.funcs, o);
    let (idioms, filtered, is) = repeated::find_idioms(&s.sources, o);
    let different: Vec<&repeated::Cluster> = clusters.iter().filter(|c| !c.same_name()).collect();
    let same: Vec<&repeated::Cluster> = clusters.iter().filter(|c| c.same_name()).collect();
    let stats = [
        format!(
            "Scan: {} files, {} functions, {} tokens, {} skipped",
            s.files,
            s.funcs.len(),
            is.tokens,
            s.skipped.len()
        ),
        format!(
            "Body clones: {} bodies signed ({} under {} tokens), {} LSH buckets ({} oversize, skipped), {} candidate pairs: {} nested in each other, {} ruled out by size, {} verified, {} accepted -> {} clusters ({} with different names)",
            cs.signed, cs.too_small, o.min_body, cs.buckets, cs.oversize, cs.candidates,
            cs.nested, cs.ruled_out_size, cs.verified, cs.accepted, clusters.len(), different.len()
        ),
        format!(
            "Idioms: {} windows of {} tokens hashed, {} recur in {}+ files, {} absorbed into a longer run, {} too generic (below {} bits, listed in the report), {} reported",
            is.windows, o.window, is.recurring, o.min_files, is.absorbed, is.low_info, o.min_info, is.reported
        ),
    ];
    let span = |x: &repeated::At| {
        if x.end_line != x.line {
            format!("{}:{}-{}", x.file, x.line, x.end_line)
        } else {
            format!("{}:{}", x.file, x.line)
        }
    };
    let mut out = format!("{REPEATED_HEADER}\n\n");
    for l in &stats {
        let _ = writeln!(out, "{l}");
    }
    let _ = writeln!(
        out,
        "\n== BODY CLONES under different names ({} clusters) ==",
        different.len()
    );
    for c in different.iter().take(a.max) {
        let locs: Vec<String> = c.fns.iter().take(4).map(|&i| loc(&s.funcs[i])).collect();
        let _ = writeln!(
            out,
            "  {}  x{} in {} files, ~{} tokens, sim>={:.2}  {}{}",
            c.names.join(" / "),
            c.fns.len(),
            c.files,
            c.size,
            c.min_sim,
            locs.join("  "),
            if c.fns.len() > 4 { "  ..." } else { "" }
        );
    }
    let _ = writeln!(out, "\n== REPEATED IDIOMS ({}) ==", idioms.len());
    for d in idioms.iter().take(a.max) {
        let locs: Vec<String> = d.at.iter().take(3).map(span).collect();
        let names: Vec<&str> = d.names.iter().take(6).map(|s| s.as_str()).collect();
        let _ = writeln!(
            out,
            "  {} files, {} tokens, info {:.1}  [{}]  {}{}",
            d.at.len(),
            d.tokens,
            d.info,
            names.join(" "),
            locs.join("  "),
            if d.at.len() > 3 { "  ..." } else { "" }
        );
    }

    let mut md = String::from("# Repeated-code candidates\n\n");
    for l in REPEATED_HEADER.lines() {
        let _ = writeln!(md, "> {l}");
    }
    md.push('\n');
    if let Some(g) = &a.generated {
        let _ = writeln!(md, "- Generated: {g}\n");
    }
    md.push_str("## Scan stats\n\n");
    for l in &stats {
        let _ = writeln!(md, "- {l}");
    }
    let clone_section = |md: &mut String, title: &str, list: &[&repeated::Cluster]| {
        let _ = writeln!(md, "\n## {title} ({})\n", list.len());
        for c in list {
            let _ = writeln!(
                md,
                "### `{}`: {} copies in {} files, ~{} tokens, sim >= {:.2}\n",
                c.names.join("` / `"),
                c.fns.len(),
                c.files,
                c.size,
                c.min_sim
            );
            for &i in &c.fns {
                let f = &s.funcs[i];
                let _ = writeln!(
                    md,
                    "- `{}:{}-{}` `{}`",
                    f.file,
                    f.line,
                    f.end_line,
                    f.qualified()
                );
            }
            md.push('\n');
        }
    };
    let idiom_section = |md: &mut String, title: &str, list: &[repeated::Idiom]| {
        let _ = writeln!(md, "\n## {title} ({})\n", list.len());
        for d in list {
            let names: Vec<String> = d.names.iter().map(|n| format!("`{n}`")).collect();
            let _ = writeln!(
                md,
                "### {} files, {} tokens, info {:.1}: {}\n\n```rust\n{}\n```\n",
                d.at.len(),
                d.tokens,
                d.info,
                if names.is_empty() {
                    "(no names)".to_string()
                } else {
                    names.join(" ")
                },
                d.excerpt.join("\n")
            );
            for x in &d.at {
                let _ = writeln!(md, "- `{}`", span(x));
            }
            md.push('\n');
        }
    };
    clone_section(&mut md, "Body clones under different names", &different);
    idiom_section(&mut md, "Repeated idioms", &idioms);
    clone_section(
        &mut md,
        "Body clones with the same name (the name finder lists these too)",
        &same,
    );
    idiom_section(
        &mut md,
        "Filtered: generic idioms (below --min-info; --min-info 0 lists them)",
        &filtered,
    );
    let _ = write!(md, "\n{WHAT_TO_DO}");
    (out, md)
}

fn main() {
    let a = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("dupcheck: {e}");
            std::process::exit(2);
        }
    };
    let s = scan(&a);
    if let Err(e) = std::fs::create_dir_all(&a.report_dir) {
        eprintln!("dupcheck: cannot create {}: {e}", a.report_dir.display());
        std::process::exit(2);
    }
    let root = repo_root();
    let write = |name: &str, (out, md): (String, String)| {
        let path = a.report_dir.join(name);
        if let Err(e) = std::fs::write(&path, md) {
            eprintln!("dupcheck: cannot write {}: {e}", path.display());
        }
        println!("{out}\nReport: {}\n", display(&path, &root));
    };
    if a.only.as_deref() != Some("repeated") {
        write("duplicate-candidates.md", names_report(&a, &s));
    }
    if a.only.as_deref() != Some("names") {
        write("repeated-code.md", repeated_report(&a, &s));
    }
}

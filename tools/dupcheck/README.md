# dupcheck: duplicated code in HeroWorld

```
scripts/dupcheck.sh                 # or: cargo run --release -p dupcheck
scripts/dupcheck.sh --help          # every option
```

It scans `crates/` (test code left out) in under a second, prints the top
candidates and writes the full lists to `reports/duplicate-candidates.md` and
`reports/repeated-code.md` (git-ignored). Like the tools it is ported from, it is
a **pointer, not a judge**: it always exits 0, and every candidate is a question
for a person, not a task.

It is a Rust port of the two duplicate finders in
[Toolbox](https://github.com/ArndRosemeier/Toolbox): `tools/duplicate-candidates`
and `tools/repeated-code`. Their READMEs explain the method and its limits in
depth; this file says what is different for Rust.

## What it finds

1. **Same-name functions across files** (`duplicate-candidates`). Three tiers,
   each pair in the strongest one: exact name, same name ignoring case and `_`,
   and near names (affixes like `get_`, `build_`, `is_` stripped, then the same
   words or an edit distance of at most 2). Each pair carries `sim`, the
   Jaccard of the two bodies' token shapes, as a hint, with prune-first early
   exits (`ruled-out (size)`, `confirmed early`, ...) as in the original.
2. **Body clones under any name** (`repeated-code`). Bodies cut into 5-token
   shingles, MinHash + LSH (16 bands x 4 rows), exact Jaccard >= 0.7,
   clustered. This is the section to read first: `seg_distance`,
   `point_segment` and `seg_dist` were one function written six times.
3. **Repeated idioms**: the same 16+ token snippet in 3+ files, with the names
   that say what the code does kept (`.field`, `.method`, `Type`, `path::item`,
   `call(`, `macro!`) and local names erased, ranked by files x specificity
   (IDF of those names). Idioms below 6 bits are generic glue and go to the
   report's "Filtered" section.

## Rust-specific choices

- Rust has a real tokenizer here: nested block comments, raw strings
  (`r#"..."#`), byte and C strings, char literals vs lifetimes, raw identifiers.
  Literals become one token, so text in a string never fakes code.
- Attributes, `use` items and `extern crate` are dropped before anything is
  compared (`#[derive(...)]` lines everywhere are not duplication).
- Test code is left out: `#[test]` and `#[cfg(test)]` items (not
  `#[cfg(not(test))]`), and `tests/` and `benches/` folders. `--tests` takes it in.
- Keywords stay in body shapes (`if`, `for`, `match`), so shapes keep their
  structure; the JS original turned them into `ID` too.
- Methods are shown with their type (`Poly::len`), from the enclosing `impl` or
  `trait`.
- The ubiquitous-name list is Rust's: trait methods (`fmt`, `from`, `default`,
  `clone`, `eq`, `hash`, `drop`, `deref`, operator traits, ...), `new` and
  `main`. Ignored pairs are counted and listed in full in the report;
  `--no-ignore` puts them back.
- Near names by edit distance need at least 6 letters after affixes are
  stripped (`NEAR_MIN_LEN`): Rust names are short, and `up` / `u16` is noise.
  Same-word matches (`road_mesh` / `build_road_mesh`) still count at any length.

## What to do with a candidate

Read both side by side and ask: if one changes, must the other change too?
If not, leave them (or rename one). If yes: keep the better one where every
caller may depend on it, point the callers at it and delete the rest; for a big
near-copy with one twist, one function with one parameter. Mind the crate
rules in AGENTS.md: shared maths goes to `hw-core` or `hw-gen`'s `geom`, never
into a crate that would pull Engine into generation or the server. When a fix
touches world generation, check that a seed still makes the same town (or bump
`WORLD_VERSION`).

Run it now and then (after a milestone, or when a file feels familiar), not
as a gate: never merge code only to make the list shorter.

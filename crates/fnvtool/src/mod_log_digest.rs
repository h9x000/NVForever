//! `fnvtool ext mod-log`: a played run's log, read down to what a mod list's tester
//! needs (fnv.ext.mod_log/1) [design: an owner's run of a 232-plugin list wrote 27,434
//! lines, about 60 of them distinct script failures; each run was read by hand].
//!
//! The log is the game's output with the test aid `NVF_PLAY_TEST_MOD_LOG=1`
//! (`nvf_play::mod_log`, lines that start with `mods:`), as `NVForever-mods-test.bat`
//! redirects it to a file. Read here: the load order's size, the scripts that do not
//! compile or run without lines, the functions NVForever lacks, the first of each script
//! failure per mod with the counts at exit, the Script Runner files skipped, the message
//! boxes shown, the plugins asked for and not there, the HUD pieces, MCM's list, the
//! movement lines, the menu files' warnings, and errors and panics. With a second log
//! (the run before), what is new and what is gone.
//!
//! The lines name plugins, scripts and functions, as the mod log does; nothing here reads
//! a mod's files.

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// HUD lines kept (a piece's line and its tiles'), and menu-file warnings shown.
const HUD_KEPT: usize = 400;
const UI_SHOWN: usize = 25;
/// The longest a line of the log is kept as (menu-file warnings join many).
const KEPT: usize = 220;
/// The line numbers one folded entry names ([`folded`]).
const LINES_NAMED: usize = 8;

/// A script failure a mod log names once: the mod (or text file) and the line's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub who: String,
    pub what: String,
}

/// What one run's log says.
#[derive(Debug, Default)]
pub struct Digest {
    pub lines: usize,
    pub first: Option<String>,
    pub last: Option<String>,
    /// The exit lines were written (a run that crashed or was killed has none).
    pub exited: bool,
    /// The mod log was on (some `mods:` line).
    pub mod_log: bool,
    pub load_order: Option<String>,
    pub units: Option<String>,
    /// ERROR lines and panics.
    pub crashes: Vec<String>,
    pub not_compiled: Vec<String>,
    pub lines_left_out: Vec<String>,
    pub notes: Vec<String>,
    /// Function → the mods that call it.
    pub missing: BTreeMap<String, BTreeSet<String>>,
    pub failures: Vec<Failure>,
    /// Mod → script errors, failed calls, skipped statements, functions not built.
    pub counts: BTreeMap<String, [u64; 4]>,
    pub runner_skipped: Vec<String>,
    pub popups: Vec<(String, usize)>,
    /// Question (`GetPluginVersion`, `IsModLoaded`, …) → the names not there.
    pub absent: BTreeMap<String, BTreeSet<String>>,
    pub hud: Vec<String>,
    pub hud_left_out: usize,
    pub mcm_lines: usize,
    pub mcm_last: Option<String>,
    pub movement: Vec<(String, usize)>,
    pub ui_warnings: Vec<(String, usize)>,
    /// Other warnings, by the part of the game that wrote them.
    pub other_warnings: BTreeMap<String, usize>,
}

/// A line without the terminal's colour codes (`ESC [ … m`), which the game wrote into a
/// redirected log until 2026-11-14.
pub fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // `[`, parameters, then one final letter.
        for d in chars.by_ref() {
            if d.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

/// A log line's time, level, writer and words; `None` for a line that is not the log
/// plugin's (the start's `setup:` lines, a panic's).
fn parts(line: &str) -> Option<(&str, &str, &str, &str)> {
    let (time, rest) = line.split_once(' ')?;
    if time.len() < 20 || !time.as_bytes()[0].is_ascii_digit() || !time.contains('T') {
        return None;
    }
    let rest = rest.trim_start();
    let (level, rest) = rest.split_once(' ')?;
    if !matches!(level, "INFO" | "WARN" | "ERROR" | "DEBUG" | "TRACE") {
        return None;
    }
    let (target, words) = rest.trim_start().split_once(": ")?;
    Some((time, level, target, words))
}

fn kept(s: &str) -> String {
    if s.chars().count() <= KEPT {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(KEPT).collect::<String>())
    }
}

fn count(list: &mut Vec<(String, usize)>, key: String) {
    match list.iter_mut().find(|(k, _)| *k == key) {
        Some((_, n)) => *n += 1,
        None => list.push((key, 1)),
    }
}

/// The number a line's words start with (`3 script errors` → 3).
fn leading(s: &str) -> u64 {
    s.trim().split(' ').next().and_then(|n| n.parse().ok()).unwrap_or(0)
}

impl Digest {
    pub fn read(text: &str) -> Digest {
        let mut d = Digest::default();
        for raw in text.lines() {
            d.lines += 1;
            let line = plain(raw);
            let Some((time, level, target, words)) = parts(&line) else {
                if line.contains("panicked at") || line.starts_with("thread '") {
                    d.crashes.push(kept(line.trim()));
                }
                continue;
            };
            d.first.get_or_insert_with(|| time.to_string());
            d.last = Some(time.to_string());
            if let Some(rest) = words.strip_prefix("mods: ") {
                d.mod_log = true;
                d.mods_line(rest);
            } else if level == "ERROR" {
                d.crashes.push(kept(&format!("{target}: {words}")));
            } else if let Some(rest) = words.strip_prefix("script runner: ") {
                if rest.contains(" skipped (") {
                    d.runner_skipped.push(kept(rest));
                }
            } else if let Some(rest) = words.strip_prefix("message box shown: ") {
                // The words without the buttons' list.
                let said = rest.rfind(" [").map_or(rest, |at| &rest[..at]);
                count(&mut d.popups, kept(said.trim()));
            } else if let Some(rest) = words.strip_prefix("movement: ") {
                count(&mut d.movement, kept(rest));
            } else if words.starts_with("scripts: ") && words.contains("units run from their source") {
                d.units = Some(words["scripts: ".len()..].to_string());
            } else if level == "WARN" {
                match words.strip_prefix("ui: ") {
                    Some(rest) => count(&mut d.ui_warnings, kept(rest)),
                    None => *d.other_warnings.entry(target.to_string()).or_default() += 1,
                }
            }
        }
        d
    }

    /// One `mods:` line (`nvf_play::mod_log` writes them).
    fn mods_line(&mut self, rest: &str) {
        if let Some(order) = rest.strip_prefix("load order, ") {
            self.load_order = Some(order.split(':').next().unwrap_or(order).to_string());
        } else if let Some(exit) = rest.strip_prefix("at exit: ") {
            self.exited = true;
            if exit.contains(" about plugins") {
                return;
            }
            // `<mod>: N script errors, N failed calls, N skipped statements, N functions not built`.
            if let Some((who, counts)) = exit.rsplit_once(": ") {
                let n: Vec<u64> = counts.split(", ").map(leading).collect();
                if let [a, b, c, e] = n[..] {
                    self.counts.insert(who.to_string(), [a, b, c, e]);
                }
            }
        } else if rest.starts_with("HUD ") {
            if self.hud.len() < HUD_KEPT {
                self.hud.push(kept(rest));
            } else {
                self.hud_left_out += 1;
            }
        } else if rest.starts_with("MCM's list") {
            self.mcm_lines += 1;
            self.mcm_last = Some(kept(rest));
        } else if let Some((_, asked)) = rest.split_once(" asks ").filter(|_| rest.contains(" (first in ")) {
            // `<who> asks <Function> "<name>": <answer>, not there (first in …)`.
            if rest.contains(", not there (first in ") {
                let function = asked.split(' ').next().unwrap_or("").to_string();
                let name = asked.split('"').nth(1).unwrap_or("").to_string();
                self.absent.entry(function).or_default().insert(name);
            }
        } else if let Some((who, tail)) = rest.split_once(": ") {
            self.mod_line(who, tail);
        }
    }

    /// A `mods: <mod>: …` line: what the mod loaded, or a failure of one of its scripts.
    fn mod_line(&mut self, who: &str, tail: &str) {
        if tail == "no scripts" || (tail.contains(" use the script extender: ") && tail.contains(" run from source")) {
            return;
        }
        if let Some(what) = tail.strip_prefix("does not compile: ") {
            self.not_compiled.push(kept(&format!("{who}: {what}")));
        } else if tail.starts_with("runs without ") {
            self.lines_left_out.push(kept(&format!("{who}: {tail}")));
        } else if tail.contains(" NVForever does not have (the calls do nothing): ") {
            for f in tail.rsplit_once(": ").map_or("", |(_, l)| l).split(", ").filter(|f| !f.is_empty()) {
                self.missing.entry(f.to_string()).or_default().insert(who.to_string());
            }
        } else if tail.contains(" NVForever has no code for: ") || tail.contains(" extender bytes and no source text to compile") {
            self.notes.push(kept(&format!("{who}: {tail}")));
        } else {
            // `<script>: <Function> is not built: its calls do nothing` counts as missing too.
            if let Some(f) = tail.strip_suffix(" is not built: its calls do nothing").and_then(|t| t.rsplit(": ").next()) {
                self.missing.entry(f.to_string()).or_default().insert(who.to_string());
            }
            // A text file's one script needs no label.
            let what = tail.replace("SCPT 0x00000000 unit 0: ", "").replace("SR 0x00000000 unit 0: ", "");
            self.failures.push(Failure { who: who.to_string(), what: kept(&what) });
        }
    }

    /// How long the run's log covers, in seconds, from the first and last lines' times of
    /// day (a run over midnight counts on).
    pub fn seconds(&self) -> Option<u64> {
        let of_day = |t: &str| -> Option<u64> {
            let clock = t.split('T').nth(1)?;
            let mut p = clock.split(':');
            let (h, m, s) = (p.next()?.parse::<u64>().ok()?, p.next()?.parse::<u64>().ok()?, p.next()?.split('.').next()?.parse::<u64>().ok()?);
            Some(h * 3600 + m * 60 + s)
        };
        let (a, b) = (of_day(self.first.as_deref()?)?, of_day(self.last.as_deref()?)?);
        Some(if b >= a { b - a } else { b + 86_400 - a })
    }

    /// The names asked for that are plugins of the script extender, not mods' files:
    /// what a mod stops over.
    fn absent_plugins(&self) -> Vec<String> {
        self.absent.iter().filter(|(f, _)| !f.eq_ignore_ascii_case("IsModLoaded") && !f.eq_ignore_ascii_case("GetModIndex")).flat_map(|(f, names)| names.iter().map(move |n| format!("{f} \"{n}\""))).collect()
    }

    /// What to compare between two runs, by kind.
    fn keys(&self) -> Vec<(&'static str, BTreeSet<String>)> {
        vec![
            ("errors and panics", self.crashes.iter().cloned().collect()),
            ("script failures", self.failures.iter().map(|f| format!("{}: {}", f.who, f.what)).collect()),
            ("message boxes", self.popups.iter().map(|(p, _)| p.clone()).collect()),
            ("Script Runner files skipped", self.runner_skipped.iter().cloned().collect()),
            ("scripts that do not compile", self.not_compiled.iter().cloned().collect()),
            ("functions NVForever does not have", self.missing.keys().cloned().collect()),
            ("extender plugins asked for and not there", self.absent_plugins().into_iter().collect()),
        ]
    }

    pub fn json(&self) -> Value {
        json!({
            "lines": self.lines,
            "first": self.first,
            "last": self.last,
            "seconds": self.seconds(),
            "exited": self.exited,
            "mod_log": self.mod_log,
            "load_order": self.load_order,
            "units": self.units,
            "errors": self.crashes,
            "not_compiled": self.not_compiled,
            "lines_left_out": self.lines_left_out,
            "notes": self.notes,
            "missing_functions": self.missing.iter().map(|(f, mods)| json!({"name": f, "mods": mods})).collect::<Vec<_>>(),
            "failures": self.failures.iter().map(|f| json!({"mod": f.who, "what": f.what})).collect::<Vec<_>>(),
            "counts": self.counts.iter().map(|(m, c)| json!({"mod": m, "script_errors": c[0], "failed_calls": c[1], "skipped_statements": c[2], "functions_not_built": c[3]})).collect::<Vec<_>>(),
            "script_runner_skipped": self.runner_skipped,
            "message_boxes": self.popups.iter().map(|(t, n)| json!({"text": t, "count": n})).collect::<Vec<_>>(),
            "not_there": self.absent.iter().map(|(f, names)| json!({"question": f, "names": names})).collect::<Vec<_>>(),
            "hud": self.hud,
            "mcm": {"lines": self.mcm_lines, "last": self.mcm_last},
            "movement": self.movement.iter().map(|(t, n)| json!({"text": t, "count": n})).collect::<Vec<_>>(),
            "ui_warnings": self.ui_warnings.iter().map(|(t, n)| json!({"text": t, "count": n})).collect::<Vec<_>>(),
            "other_warnings": self.other_warnings,
        })
    }
}

/// What a run has that the run before had not (`new`), and the other way (`gone`), by
/// kind; kinds with neither are left out.
pub fn changes(now: &Digest, before: &Digest) -> Vec<(&'static str, Vec<String>, Vec<String>)> {
    now.keys()
        .into_iter()
        .zip(before.keys())
        .filter_map(|((kind, a), (_, b))| {
            let new: Vec<String> = a.difference(&b).cloned().collect();
            let gone: Vec<String> = b.difference(&a).cloned().collect();
            (!new.is_empty() || !gone.is_empty()).then_some((kind, new, gone))
        })
        .collect()
}

pub fn changes_json(c: &[(&'static str, Vec<String>, Vec<String>)]) -> Value {
    Value::Array(c.iter().map(|(kind, new, gone)| json!({"kind": kind, "new": new, "gone": gone})).collect())
}

/// Entries that differ only in a script's line number, as one entry naming the lines
/// (`lines 1, 2, 3: not built yet: …`), in the order they first came; at most
/// [`LINES_NAMED`] numbers are written out.
fn folded(items: &[String]) -> Vec<String> {
    // Where an entry's `line N: ` stands: (start, the number, the end after `: `).
    let at = |s: &str| -> Option<(usize, u64, usize)> {
        let mut from = 0;
        while let Some(i) = s[from..].find("line ") {
            let start = from + i;
            let digits: String = s[start + 5..].chars().take_while(char::is_ascii_digit).collect();
            let end = start + 5 + digits.len();
            if !digits.is_empty() && s[end..].starts_with(": ") && (start == 0 || s[..start].ends_with(' ')) {
                return Some((start, digits.parse().ok()?, end + 2));
            }
            from = start + 5;
        }
        None
    };
    let mut groups: Vec<(String, String, Vec<u64>)> = Vec::new();
    for item in items {
        let (head, tail, n) = match at(item) {
            Some((a, n, b)) => (item[..a].to_string(), item[b..].to_string(), Some(n)),
            None => (item.clone(), String::new(), None),
        };
        match (n, groups.iter_mut().find(|(h, t, ns)| *h == head && *t == tail && !ns.is_empty())) {
            (Some(n), Some(g)) => g.2.push(n),
            (n, _) => groups.push((head, tail, n.into_iter().collect())),
        }
    }
    groups
        .into_iter()
        .map(|(head, tail, lines)| match lines.len() {
            0 => head,
            1 => format!("{head}line {}: {tail}", lines[0]),
            n => {
                let named: Vec<String> = lines.iter().take(LINES_NAMED).map(u64::to_string).collect();
                let more = if n > LINES_NAMED { format!(" and {} more", n - LINES_NAMED) } else { String::new() };
                format!("{head}lines {}{more}: {tail}", named.join(", "))
            }
        })
        .collect()
}

fn section(out: &mut String, title: &str, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    out.push_str(&format!("\n== {title} ({})\n", lines.len()));
    for l in lines {
        out.push_str(&format!("  {l}\n"));
    }
}

/// The digest as a text a person reads: what changed since the run before first, then
/// each kind under its own heading, the mods with the most failures first.
pub fn text(now: &Digest, before: Option<&Digest>) -> String {
    let mut out = String::from("NVForever mod test run\n");
    let time = match (now.first.as_deref(), now.seconds()) {
        (Some(t), Some(s)) => format!("started {t}, {} min {} s of log", s / 60, s % 60),
        _ => "no timed lines".to_string(),
    };
    out.push_str(&format!("{} lines; {time}\n", now.lines));
    if let Some(l) = &now.load_order {
        out.push_str(&format!("load order: {l}\n"));
    }
    if let Some(u) = &now.units {
        out.push_str(&format!("scripts: {u}\n"));
    }
    if !now.mod_log {
        out.push_str("The log has no `mods:` lines: the game was not started with NVF_PLAY_TEST_MOD_LOG=1 (NVForever-mods-test.bat sets it), or stopped before them.\n");
    } else if !now.exited {
        out.push_str("The exit lines are missing: the game crashed, was closed from outside, or is still running. The counts per mod are not known.\n");
    }
    if let Some(b) = before {
        let c = changes(now, b);
        out.push_str("\n== Since the run before\n");
        if c.is_empty() {
            out.push_str("  nothing new and nothing gone (failures, message boxes, skipped files, missing functions and plugins are the same)\n");
        }
        for (kind, new, gone) in &c {
            out.push_str(&format!("  {kind}: {} new, {} gone\n", new.len(), gone.len()));
            for n in folded(new) {
                out.push_str(&format!("    + {n}\n"));
            }
            for g in folded(gone) {
                out.push_str(&format!("    - {g}\n"));
            }
        }
    }
    section(&mut out, "Errors and panics", &now.crashes);
    section(&mut out, "Message boxes shown", &now.popups.iter().map(|(t, n)| if *n > 1 { format!("{t}  (x{n})") } else { t.clone() }).collect::<Vec<_>>());
    section(&mut out, "Script Runner files skipped", &now.runner_skipped);
    // Failures by mod, the mods with the most at exit first.
    let total = |m: &str| now.counts.get(m).map_or(0, |c| c.iter().sum::<u64>());
    let mut mods: Vec<&str> = Vec::new();
    for f in &now.failures {
        if !mods.contains(&f.who.as_str()) {
            mods.push(&f.who);
        }
    }
    mods.sort_by_key(|m| std::cmp::Reverse(total(m)));
    if !mods.is_empty() {
        out.push_str(&format!("\n== Script failures ({} distinct in {} mods and files; the first of each kind, with the counts at exit)\n", now.failures.len(), mods.len()));
        for m in mods {
            match now.counts.get(m) {
                Some(c) => out.push_str(&format!("  {m}  [{} script errors, {} failed calls, {} skipped statements, {} functions not built]\n", c[0], c[1], c[2], c[3])),
                None => out.push_str(&format!("  {m}\n")),
            }
            let whats: Vec<String> = now.failures.iter().filter(|f| f.who == m).map(|f| f.what.clone()).collect();
            for what in folded(&whats) {
                out.push_str(&format!("      {what}\n"));
            }
        }
    }
    section(&mut out, "Scripts that do not compile and cannot run", &now.not_compiled);
    section(&mut out, "Scripts that run without some lines", &now.lines_left_out);
    section(&mut out, "Functions NVForever does not have", &now.missing.iter().map(|(f, mods)| format!("{f}  ({})", mods.iter().cloned().collect::<Vec<_>>().join(", "))).collect::<Vec<_>>());
    section(&mut out, "Other notes at load", &now.notes);
    section(&mut out, "Extender plugins asked for and not there", &now.absent_plugins());
    let files: usize = now.absent.iter().filter(|(f, _)| f.eq_ignore_ascii_case("IsModLoaded") || f.eq_ignore_ascii_case("GetModIndex")).map(|(_, n)| n.len()).sum();
    if files > 0 {
        out.push_str(&format!("\n== Mod files asked for and not there: {files} (optional patches as a rule; the names are in the log's exit line)\n"));
    }
    section(&mut out, "Movement", &now.movement.iter().map(|(t, n)| if *n > 1 { format!("{t}  (x{n})") } else { t.clone() }).collect::<Vec<_>>());
    if now.mcm_lines > 0 {
        out.push_str(&format!("\n== MCM's list: {} lines; the last\n  {}\n", now.mcm_lines, now.mcm_last.as_deref().unwrap_or("")));
    }
    if !now.hud.is_empty() {
        out.push_str(&format!("\n== HUD pieces ({} lines{})\n", now.hud.len(), if now.hud_left_out > 0 { format!(", {} more in the log", now.hud_left_out) } else { String::new() }));
        for l in &now.hud {
            out.push_str(&format!("  {l}\n"));
        }
    }
    if !now.ui_warnings.is_empty() {
        out.push_str(&format!("\n== Menu files' warnings ({} distinct; the first {})\n", now.ui_warnings.len(), UI_SHOWN.min(now.ui_warnings.len())));
        for (t, n) in now.ui_warnings.iter().take(UI_SHOWN) {
            out.push_str(&format!("  {t}{}\n", if *n > 1 { format!("  (x{n})") } else { String::new() }));
        }
    }
    section(&mut out, "Other warnings, by the part of the game", &now.other_warnings.iter().map(|(t, n)| format!("{t}: {n}")).collect::<Vec<_>>());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A log in the game's format: the words are made up, no mod's.
    fn log(extra: &[&str]) -> String {
        let mut lines = vec![
            "setup: plugins: FalloutNV.esm, A.esp".to_string(),
            "\u{1b}[2m2026-11-14T23:59:50.000001Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \u{1b}[2mnvf_play::mod_log\u{1b}[0m\u{1b}[2m:\u{1b}[0m mods: load order, 3 plugins (2 mods): 00 FalloutNV.esm, 01 A.esp, 02 B.esp".to_string(),
        ];
        let t = |s: &str| format!("2026-11-14T23:59:51.000000Z {s}");
        lines.extend(
            [
                " INFO nvf_play::script: scripts: 7 units run from their source text, 1 kept their bytes (the source does not compile)",
                " INFO nvf_play::mod_log: mods: A.esp: 4 scripts, 3 use the script extender: 2 run from source, 1 does not compile and cannot run",
                " INFO nvf_play::mod_log: mods: A.esp: does not compile: SCPT 0x01000802 (line 12: syntax)",
                " INFO nvf_play::mod_log: mods: A.esp: runs without 1 line that do not compile: SCPT 0x01000803 line 7: syntax",
                " INFO nvf_play::mod_log: mods: A.esp: calls 2 functions NVForever does not have (the calls do nothing): NoSuch, Other",
                " INFO nvf_play::mod_log: mods: B.esp: calls 1 function NVForever does not have (the calls do nothing): NoSuch",
                " INFO nvf_play::mod_log: mods: B.esp: no scripts",
                " WARN nvf_play::script: script runner: ln_x.txt skipped (new game): line 19: syntax: missing )",
                " INFO nvf_play::script: script runner: gr_y.txt ran (game restart)",
                " WARN nvf_play::mod_log: mods: the console asks GetPluginVersion \"yUI\": -1, not there (first in SR 0x00000000 unit 0)",
                " WARN nvf_play::mod_log: mods: a.esp asks IsModLoaded \"Patch.esp\": 0, not there (first in SCPT 0x01000801)",
                " INFO nvf_play::mod_log: mods: a.esp asks IsPluginInstalled \"JIP NVSE Plugin\": 1 (first in SCPT 0x01000801)",
                " WARN nvf_play::mod_log: mods: A.esp: SCPT 0x01000801: skipped: line 66: a string where a number is expected (use #)",
                " WARN nvf_play::mod_log: mods: CompileScript M\\R.gek: SCPT 0x00000000 unit 0: skipped: line 3: no element at key \"k\"",
                " WARN nvf_play::mod_log: mods: A.esp: SCPT 0x01000801: SetThing is not built: its calls do nothing",
                " INFO nvf_play::script_hud: message box shown: A needs B! / Install B. [\"Ok\"]",
                " INFO nvf_play::script_hud: message box shown: A needs B! / Install B. [\"Ok\"]",
                " INFO nvf_play::combat: movement: the Modify Run Speed perks make the player move at 175% (TestPerk)",
                " INFO nvf_play::mod_log: mods: MCM's list: _selected -1, rows on: none",
                " INFO nvf_play::mod_log: mods: HUD piece HUDMainMenu/Loot shows: 2 tiles",
                " INFO nvf_play::mod_log: mods: HUD   HUDMainMenu/Loot rect visible 1",
                " WARN nvf_play::tiles: ui: menus\\x.xml: line 4: <a> closed by </b>",
                " WARN nvforever::sky: sky: no cloud texture",
            ]
            .iter()
            .map(|s| t(s)),
        );
        lines.extend(extra.iter().map(|s| s.to_string()));
        lines.join("\r\n")
    }

    const EXIT: [&str; 2] = [
        "2026-11-15T00:01:00.500000Z  INFO nvf_play::mod_log: mods: at exit: A.esp: 0 script errors, 1 failed calls, 511 skipped statements, 2 functions not built",
        "2026-11-15T00:01:00.600000Z  INFO nvf_play::mod_log: mods: at exit: 3 questions about plugins, asked for and not there: GetPluginVersion \"yUI\" (the console)",
    ];

    #[test]
    fn a_run_is_read_down_to_what_its_mods_ran_into() {
        let d = Digest::read(&log(&EXIT));
        assert_eq!(d.load_order.as_deref(), Some("3 plugins (2 mods)"));
        assert_eq!(d.seconds(), Some(70), "over midnight");
        assert!(d.exited && d.mod_log);
        assert_eq!(d.not_compiled, ["A.esp: SCPT 0x01000802 (line 12: syntax)"]);
        assert_eq!(d.lines_left_out.len(), 1);
        assert_eq!(d.missing.keys().collect::<Vec<_>>(), ["NoSuch", "Other", "SetThing"]);
        assert_eq!(d.missing["NoSuch"].len(), 2, "both mods");
        assert_eq!(d.runner_skipped, ["ln_x.txt skipped (new game): line 19: syntax: missing )"]);
        assert_eq!(d.popups, [("A needs B! / Install B.".to_string(), 2)]);
        assert_eq!(d.absent_plugins(), ["GetPluginVersion \"yUI\""]);
        assert_eq!(d.absent["IsModLoaded"].len(), 1);
        // A text file's script has no label; a plugin's keeps its own.
        let failures: Vec<String> = d.failures.iter().map(|f| format!("{}|{}", f.who, f.what)).collect();
        assert_eq!(
            failures,
            [
                "A.esp|SCPT 0x01000801: skipped: line 66: a string where a number is expected (use #)",
                "CompileScript M\\R.gek|skipped: line 3: no element at key \"k\"",
                "A.esp|SCPT 0x01000801: SetThing is not built: its calls do nothing",
            ]
        );
        assert_eq!(d.counts["A.esp"], [0, 1, 511, 2]);
        assert_eq!((d.mcm_lines, d.hud.len(), d.ui_warnings.len()), (1, 2, 1));
        assert_eq!(d.other_warnings["nvforever::sky"], 1);
        let text = text(&d, None);
        assert!(text.contains("A.esp  [0 script errors, 1 failed calls, 511 skipped statements, 2 functions not built]"), "{text}");
        assert!(text.contains("NoSuch  (A.esp, B.esp)"), "{text}");
        assert!(!text.contains('\u{1b}'));
        assert_eq!(d.json()["failures"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn failures_that_differ_only_in_the_line_are_one_entry() {
        let items: Vec<String> = ["A: SCPT 1: skipped: line 3: not built: F", "A: SCPT 1: skipped: line 9: not built: F", "A: SCPT 1: skipped: line 4: other", "B: no line here", "A: SCPT 1: skipped: line 12: not built: F"].iter().map(|s| s.to_string()).collect();
        assert_eq!(folded(&items), ["A: SCPT 1: skipped: lines 3, 9, 12: not built: F", "A: SCPT 1: skipped: line 4: other", "B: no line here"]);
        let many: Vec<String> = (1..=11).map(|n| format!("x: line {n}: y")).collect();
        assert_eq!(folded(&many), ["x: lines 1, 2, 3, 4, 5, 6, 7, 8 and 3 more: y"]);
        // `outline 3: ` is no line number.
        assert_eq!(folded(&["an outline 3: x".to_string(), "an outline 4: x".to_string()]).len(), 2);
    }

    #[test]
    fn a_run_that_stopped_early_says_so() {
        let d = Digest::read(&log(&["thread 'main' panicked at src\\x.rs:1:1:", "2026-11-15T00:00:10.000000Z ERROR nvforever::crash: crash: a panic"]));
        assert!(!d.exited);
        assert_eq!(d.crashes.len(), 2);
        assert!(text(&d, None).contains("The exit lines are missing"));
        // A log of a run without the aid.
        let plain = Digest::read("2026-11-15T00:00:10.000000Z  INFO nvforever::sky: sky: clear");
        assert!(text(&plain, None).contains("no `mods:` lines"));
    }

    #[test]
    fn two_runs_are_compared_by_what_is_new_and_gone() {
        let before = Digest::read(&log(&EXIT));
        let now_text = log(&EXIT)
            .replace("line 66: a string where a number is expected (use #)", "line 70: no element at key \"x\"")
            .replace(" INFO nvf_play::script_hud: message box shown: A needs B! / Install B. [\"Ok\"]", " INFO nvf_play::sky: sky: clear")
            .replace(" WARN nvf_play::script: script runner: ln_x.txt skipped (new game): line 19: syntax: missing )", " INFO nvf_play::script: script runner: ln_x.txt ran (new game)");
        let now = Digest::read(&now_text);
        let c = changes(&now, &before);
        let kinds: Vec<&str> = c.iter().map(|(k, _, _)| *k).collect();
        assert_eq!(kinds, ["script failures", "message boxes", "Script Runner files skipped"]);
        assert_eq!((c[0].1.len(), c[0].2.len()), (1, 1));
        assert_eq!((c[1].1.len(), c[1].2.len()), (0, 1));
        let text = text(&now, Some(&before));
        assert!(text.contains("== Since the run before"), "{text}");
        assert!(text.contains("    + A.esp: SCPT 0x01000801: skipped: line 70: no element at key \"x\""), "{text}");
        assert!(text.contains("    - A needs B! / Install B."), "{text}");
        assert!(super::text(&before, Some(&before)).contains("nothing new and nothing gone"));
        assert_eq!(changes_json(&c)[0]["kind"], "script failures");
    }
}

//! The connection check: every function a load order's scripts can call, and whether the
//! engine has it, without running the scripts (`fnvtool ext connections`).
//!
//! A mod's scripts name every function they can ever call, the ones behind a key press or
//! a late quest stage too. Starting the game reaches few of them; this walks all of them
//! in the units as the game built them (compiled from source where the game does that,
//! `crate::source_compile`) and gives each one a [`Status`]:
//!
//! - a numbered function by what is registered for its opcode (`crate::functions`): an
//!   implementation, a stub (it answers and changes nothing, its system is not built),
//!   nothing to do on this engine, or nothing registered at all;
//! - a function reached by name (the extenders', and the base game's that the catalogue
//!   has no opcode for) by calling it once with placeholder arguments, as the extender
//!   census does ([`probe`]): it answers, or it is "not built yet";
//! - a name nothing knows (no catalogue entry): it compiled as a call that does nothing
//!   (`fnv_script::compile::Seed::unknown_calls`).
//!
//! What it does not see [design]: a function that answers and whose effect never reaches
//! the engine's systems (only the registered stubs are marked), a call that fails on the
//! arguments a script really passes, and anything drawn.

use crate::game::{Game, RunCtx};
use fnv_script::table::FunctionTable;
use fnv_script::Program;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// What the engine does with a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Implemented.
    Built,
    /// Deliberately nothing to do on this engine (`functions::Kind::NoOp`).
    NoOp,
    /// It answers and has no effect: its system is not built (`functions::Kind::Stub`).
    Stub,
    /// A numbered function nothing is registered for: the call reads 0.
    Unhandled,
    /// A function reached by name that no module answers: the statement is skipped.
    NotBuilt,
    /// A name with no catalogue entry: the call does nothing.
    Unknown,
}

impl Status {
    pub const BROKEN: [Status; 4] = [Status::Stub, Status::Unhandled, Status::NotBuilt, Status::Unknown];

    /// Whether a script calling it gets less than the original gives.
    pub fn broken(self) -> bool {
        Self::BROKEN.contains(&self)
    }
}

/// The catalogue's rows that are statements, not functions: syntax, never probed.
pub const STATEMENTS: [&str; 8] = ["let", "while", "loop", "foreach", "break", "continue", "eval", "search"];

/// A function a program calls: by opcode, or by name where it has none.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Callee {
    Opcode(u16),
    Name(String),
}

/// Every call in a program with its count, and the statements the decoder could not read
/// (a unit that kept its bytes): the walk is over the program as serialised, where a call
/// statement or expression token has `opcode` and `has_params`, an extender expression's
/// call has `func` (`by`, `id`), and an unread statement `opcode` and `body`.
pub fn callees(program: &Program) -> (BTreeMap<Callee, usize>, usize) {
    fn walk(v: &serde_json::Value, out: &mut BTreeMap<Callee, usize>, unread: &mut usize) {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(f) = m.get("func") {
                    let callee = match (f["by"].as_str(), &f["id"]) {
                        (Some("opcode"), id) => id.as_u64().map(|n| Callee::Opcode(n as u16)),
                        (Some("name"), id) => id.as_str().map(|s| Callee::Name(s.to_string())),
                        _ => None,
                    };
                    if let Some(c) = callee {
                        *out.entry(c).or_default() += 1;
                    }
                } else if let Some(op) = m.get("opcode").and_then(serde_json::Value::as_u64) {
                    if m.contains_key("has_params") {
                        *out.entry(Callee::Opcode(op as u16)).or_default() += 1;
                    } else if m.contains_key("body") {
                        *unread += 1;
                    }
                }
                m.values().for_each(|x| walk(x, out, unread));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out, unread)),
            _ => {}
        }
    }
    let mut out = BTreeMap::new();
    let mut unread = 0;
    if let Ok(v) = serde_json::to_value(program) {
        walk(&v, &mut out, &mut unread);
    }
    (out, unread)
}

/// The status of a numbered function: what `crate::functions` registers for its opcode,
/// or under its catalogue name (the console's functions have numbers of the table's own).
pub fn opcode_status(table: &FunctionTable, opcode: u16) -> (Status, Option<&'static str>) {
    use crate::functions::{lookup, lookup_name, Kind};
    let kind = lookup(opcode).or_else(|| lookup_name(&table.get(opcode)?.name));
    match kind {
        Some(Kind::Impl(_)) => (Status::Built, None),
        Some(Kind::NoOp(why)) => (Status::NoOp, Some(why)),
        Some(Kind::Stub(why)) => (Status::Stub, Some(why)),
        None => (Status::Unhandled, None),
    }
}

/// Whether a function reached by name answers: called once with a placeholder for each
/// parameter kind the catalogue lists (0, PlayerREF, "a", Luck, X), then with fewer while
/// the compiler takes fewer than the catalogue lists. `None`: no argument count compiled,
/// with what the compiler said. The extender census (`tests/synthetic.rs`) counts by it.
pub fn probe(g: &mut Game<'_>, name: &str, kinds: &[&str]) -> Result<bool, String> {
    let args: Vec<&str> = kinds
        .iter()
        .map(|k| match *k {
            "form" | "reference" => "PlayerREF",
            "string" => "\"a\"",
            "actor_value" => "Luck",
            "axis" => "X",
            _ => "0",
        })
        .collect();
    let mut said_last = String::new();
    for k in (0..=args.len()).rev() {
        let before = g.events.len();
        let r = g.run_query(&format!("Begin Function {{ }}\n{name} {}\nEnd\n", args[..k].join(" ")), RunCtx::default());
        let said: Vec<String> = g.events[before..].iter().map(|e| format!("{e:?}")).collect();
        if r.as_ref().is_err_and(|e| e.contains("did not compile")) {
            said_last = said.join(" | ");
            continue;
        }
        return Ok(!(said.iter().any(|e| e.contains("not built")) || r.as_ref().is_err_and(|e| e.contains("not built"))));
    }
    Err(said_last)
}

/// One function as one plugin's scripts use it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Use {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opcode: Option<String>,
    /// The catalogue's origin (`GECK1`, `NVSE`, `JIP`, …); empty for a name nothing knows.
    pub origin: String,
    pub status: Status,
    /// A stub's or no-op's stated reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// Units calling it, and call sites in them.
    pub units: usize,
    pub calls: usize,
}

/// One plugin's scripts.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PluginUse {
    pub plugin: String,
    pub units: usize,
    /// Units whose source did not compile: they kept bytes the engine cannot run, and
    /// their calls are not in the counts.
    pub units_kept_bytes: usize,
    /// Statements the decoder could not read, in any unit.
    pub unread_statements: usize,
    pub call_sites: usize,
    pub functions: usize,
    pub by_status: BTreeMap<Status, usize>,
    /// The functions that are not built or do nothing, most used first.
    pub broken: Vec<Use>,
}

/// A broken function over the whole load order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Broken {
    pub name: String,
    pub origin: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    pub plugins: Vec<String>,
    pub units: usize,
    pub calls: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub plugins: Vec<PluginUse>,
    /// Every broken function, by the number of plugins calling it, then units.
    pub broken: Vec<Broken>,
    /// Names that could not be probed: no argument count compiled.
    pub not_probed: Vec<String>,
}

/// Per callee: the units calling it and the call sites in them.
type Sites = BTreeMap<Callee, (usize, usize)>;

/// The check over the game's load order. `only`: the plugins to report (lower case); none
/// reports all. The game is used for the probes only: start it from a new game first.
pub fn check(g: &mut Game<'_>, only: &BTreeSet<String>) -> Report {
    let data = g.data;
    let table = &data.table;
    let kept: BTreeSet<&str> = data.source_units.iter().filter(|u| u.error.is_some()).map(|u| u.key.as_str()).collect();
    let unknown: BTreeSet<String> = data.source_units.iter().flat_map(|u| u.unknown_functions.iter()).map(|n| n.to_ascii_lowercase()).collect();
    // Per plugin, per callee: (units, calls).
    let mut per: BTreeMap<String, (PluginUse, Sites)> = BTreeMap::new();
    for c in &data.units {
        let plugin = &c.unit.plugin;
        if !only.is_empty() && !only.contains(&plugin.to_ascii_lowercase()) {
            continue;
        }
        let entry = per.entry(plugin.clone()).or_insert_with(|| {
            (PluginUse { plugin: plugin.clone(), units: 0, units_kept_bytes: 0, unread_statements: 0, call_sites: 0, functions: 0, by_status: BTreeMap::new(), broken: Vec::new() }, BTreeMap::new())
        });
        entry.0.units += 1;
        if kept.contains(c.key().as_str()) {
            entry.0.units_kept_bytes += 1;
            continue;
        }
        let (calls, unread) = callees(&c.program);
        entry.0.unread_statements += unread;
        for (callee, n) in calls {
            let e = entry.1.entry(callee).or_default();
            e.0 += 1;
            e.1 += n;
        }
    }
    // Each name once.
    let mut named: BTreeMap<String, Result<bool, String>> = BTreeMap::new();
    let names: BTreeSet<String> = per.values().flat_map(|(_, m)| m.keys()).filter_map(|c| if let Callee::Name(n) = c { Some(n.to_ascii_lowercase()) } else { None }).collect();
    for low in names {
        if STATEMENTS.contains(&low.as_str()) {
            continue;
        }
        let Some(u) = table.unnumbered.get(&low) else { continue };
        let kinds: Vec<&str> = u.params.iter().map(|p| p.kind.as_str()).collect();
        let verdict = probe(g, &u.name, &kinds);
        named.insert(low, verdict);
    }
    let mut not_probed = BTreeSet::new();
    let mut all: BTreeMap<(String, Status), Broken> = BTreeMap::new();
    let mut plugins = Vec::new();
    for (_, (mut p, calls)) in per {
        for (callee, (units, n)) in calls {
            let (name, opcode, origin, status, why) = match &callee {
                Callee::Opcode(op) => {
                    let f = table.get(*op);
                    let (status, why) = opcode_status(table, *op);
                    let name = f.map_or_else(|| format!("opcode 0x{op:04X}"), |f| f.name.clone());
                    (name, Some(format!("0x{op:04X}")), f.and_then(|f| f.origin.clone()).unwrap_or_default(), status, why)
                }
                Callee::Name(n) => {
                    let low = n.to_ascii_lowercase();
                    match (table.unnumbered.get(&low), named.get(&low)) {
                        (Some(u), Some(Ok(true))) => (u.name.clone(), None, u.origin.clone(), Status::Built, None),
                        (Some(u), Some(Ok(false))) => (u.name.clone(), None, u.origin.clone(), Status::NotBuilt, None),
                        // A statement's keyword where a call stands (`(let a += 1)` in an
                        // expression once compiled so): nothing answers it at run time.
                        (Some(u), _) if STATEMENTS.contains(&low.as_str()) => (u.name.clone(), None, u.origin.clone(), Status::NotBuilt, Some("a statement's keyword compiled as a call")),
                        (Some(u), _) => {
                            not_probed.insert(u.name.clone());
                            (u.name.clone(), None, u.origin.clone(), Status::Built, None)
                        }
                        (None, _) if unknown.contains(&low) => (n.clone(), None, String::new(), Status::Unknown, None),
                        // The VM's own functions (strings, arrays, the statements).
                        (None, _) => (n.clone(), None, String::new(), Status::Built, None),
                    }
                }
            };
            p.call_sites += n;
            p.functions += 1;
            *p.by_status.entry(status).or_default() += 1;
            if status.broken() {
                let why = why.map(str::to_string);
                p.broken.push(Use { name: name.clone(), opcode, origin: origin.clone(), status, why: why.clone(), units, calls: n });
                let b = all.entry((name.to_ascii_lowercase(), status)).or_insert_with(|| Broken { name, origin, status, why, plugins: Vec::new(), units: 0, calls: 0 });
                b.plugins.push(p.plugin.clone());
                b.units += units;
                b.calls += n;
            }
        }
        p.broken.sort_by(|a, b| b.units.cmp(&a.units).then_with(|| b.calls.cmp(&a.calls)).then_with(|| a.name.cmp(&b.name)));
        plugins.push(p);
    }
    let mut broken: Vec<Broken> = all.into_values().collect();
    broken.sort_by(|a, b| b.plugins.len().cmp(&a.plugins.len()).then_with(|| b.units.cmp(&a.units)).then_with(|| a.name.cmp(&b.name)));
    Report { plugins, broken, not_probed: not_probed.into_iter().collect() }
}

# Testing real mods: the connection check, the script report and the mod log

For sessions and testers checking how mods that need the script extender run on
NVForever. Three tools, what each sees, and how they work together. Written 2026-11-13
from the mod tests of 2026-11-12 (issues #2268, #2270, #2271, #2272, #2273).

Rules that hold for all of them: mods are read where the tester has them and never
committed; the tools print names and counts, no script text (`ext report --detail` is the
one exception and its output stays private); no extender source is read.

## The layers a mod has to pass

A mod works only when every layer holds, and a player sees nothing until the last one does.

| Layer | Question | Tool that answers it | Needs the game running |
|---|---|---|---|
| 1 | Does the game start with the mod (archives, plugins)? | the run log's first lines | yes |
| 2 | Do its scripts compile? | `fnvtool ext report` | no |
| 3 | Does every function its scripts can call exist? | `fnvtool ext connections` | no |
| 4 | Does what it calls work when it runs (arguments, state)? | the mod log | yes |
| 5 | Does a call's effect reach the game? | nothing yet (issue #2272) | yes |
| 6 | Are its menus and HUD pieces drawn, and do they take input? | the mod log's MCM lines; otherwise a look at the screen | yes |

## 1. `fnvtool ext connections`: every call, without running anything

```
fnvtool ext connections <Data> <masters…> <mod plugins by full path…> --schema schemas/fnv [--only <plugin>…]
```

Walks every unit of the load order as the game built it and gives every function it can
call a status (`fnv_game::connections`, format `fnv.ext.connections/1`,
docs/cli-contract.md):

- `built`, `no_op` (deliberately nothing to do here): fine.
- `stub`: it answers and changes nothing, its system is not built.
- `unhandled`: a numbered function nothing is registered for; the call reads 0.
- `not_built`: a function reached by name that no module answers; the statement is skipped.
- `unknown`: a name with no catalogue entry (a plugin's own undocumented function); the
  call does nothing.

`broken` lists the last four over the whole load order, by the number of plugins calling
each: the ranked list to fix from. It covers the code behind a key press or a late quest
stage as well as the start-up scripts, which a played run reaches only by chance. About
two seconds for the base game plus ten mods.

It does not see: a function that answers but whose effect never reaches the engine (only
registered stubs are marked), a call that fails on the arguments really passed, anything
drawn.

## 2. `fnvtool ext report`: which scripts do not compile

```
fnvtool ext report <Data> <masters…> <mod plugins…> --schema schemas/fnv [--detail]
```

Per plugin: the units that run from their source, the units that kept their bytes with the
line and kind of their first error, the functions with no catalogue entry, the
enumeration names the table lacks (`fnv.ext.report/1`). A unit that kept its bytes cannot
run, and `ext connections` cannot count its calls (`units_kept_bytes`): fix those first.
Since 2026-11-13 a call, `set`, `let` or assignment line that does not compile is left out
instead of the whole unit (`lines_dropped`, with line and error kind): the unit runs, but
without those lines, so they are the next thing to fix. The mod log names them too
(`runs without N lines that do not compile`).

## 3. The mod log: what a played run met

Start the game with `NVF_PLAY_TEST_MOD_LOG=1` (`nvf_play::mod_log`). It is a measuring aid:
the run stays a hand-played one. The log then has `mods:` lines:

- at the start, the load order and per mod what `ext report` says;
- each once, what a mod's scripts ask about plugins and read (IsPluginInstalled,
  GetPluginVersion, IsDLLLoaded, IsModLoaded, GetModIndex). A "not there" is usually why a
  mod switches itself off;
- the first of each failure in a mod's script: function not built, failed call, skipped
  statement, script stopped early. In a script compiled from source (a mod's recompiled
  script, a Script Runner file, a CompileScript file) the failure starts with `line N:`,
  the line of the mod's text, so the cause can be read there;
- under the pause menu's Mods page, MCM's list and option panel whenever they change;
- `HUD piece …` blocks: each HUD element a mod adds (an injected or loose tile of
  `HUDMainMenu` with everything under it) when it first shows, and again when its tiles
  change while it shows (up to four times, half a second apart at least): each tile's
  kind, `visible`, `x`, `y`, `width`, `height`, `alpha`, `depth`, `string`, `filename`.
  A piece that draws wrong or empty is told from one never built: rows a script made show
  up as tiles; missing ones were never made. When a mod replaced the whole menu file (a
  loose `hud_main_menu.xml`), each of its parts is a piece of its own, written only when its
  tiles change while it shows (rows a script builds);
- a CompileScript file is named in its lines (`CompileScript <path>`), and one that does
  not compile says so with its line (`CompileScript <path>|compile`);
- at exit, counts per mod and what was asked for and not there.

## How they fit

1. **Before any run**: `ext report`, then `ext connections`, on the mod list. Everything
   they list is known without playing, for any number of mods at once.
2. **Then play** with the mod log on, and use what the mod adds.
3. **Read the log against the check.** A "not built" or "unknown" line in the log is a
   function the check already listed: it tells which of the listed gaps the played part
   of the mod actually hit, and how often. A log failure of another kind (a failed call,
   a skipped statement that is not "not built", a question answered "not there", a
   script error) is something only running shows: those are the new findings.
4. **A function in the log's "not built" lines that the check did not list** is a fault
   in the check or the compiler (2026-11-13: `let` inside an expression compiled as a
   call of the catalogue's `Let`; found that way and fixed).
5. **Nothing in either, and the feature still does nothing**: layer 5 or 6. Say what was
   pressed and what was seen; no tool sees that yet.

## Counting over the whole catalogue

`cargo test --release -p fnv-game --test synthetic -- --ignored extender_census --nocapture`
calls every extender function of the catalogue once and lists the unbuilt
(`target/tmp/extender-census.txt`); with `NVF_CENSUS_FAMILY=vanilla` it does the same for
the base game's functions the catalogue has no opcode for (`vanilla-census.txt`). The
connection check probes a function reached by name the same way
(`fnv_game::connections::probe`), for the functions a load order really calls.

## Not built yet

- A driver that fires every entry point of a mod headless (each block, key, event
  handler, function script), so the log records failures without anyone playing.
- A structured report of a played run instead of log lines, and a marker key for "it went
  wrong here".
- A check that a call's effect reaches the engine's systems (issue #2272).

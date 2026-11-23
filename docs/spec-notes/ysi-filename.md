# ySI filename configuration subset

The original Sorting Icons configuration's author documentation describes `tags` and
`icons` sections, additional JSON files contributing those sections, highest-priority
matching tag rules, form/property conditions and explicit mod/form/formlist selectors.
Icon records associate a tag with its template and filename [documented: Sorting
Icons 1.4b package, `menus/ySI/ySI.json` author documentation; original configuration
is not redistributed].

`yUI` (version 140) registers `ySIGetTrait` (`0x21D0`) and `ySISetTrait` (`0x21D1`)
[documented: GECK wiki `YSIGetTrait`/`YSISetTrait`; observed: original FNV 1.4.0.525
with xNVSE 6.4.9 and yUI 140]. In that original-game runtime, `ySIGetTagTrait` is not
registered by any loaded plugin, and calling `ySIGetTrait "filename"` on an item
configured with a tag and icon filename (`FalloutNV.esm` `00000F`) returns the
configured icon filename string while passing a string tag instead of a form fails
script compilation [observed: original FNV 1.4.0.525 with xNVSE 6.4.9 and yUI 140].
Accordingly, `ySIGetTagTrait` is left out of NVForever, and the bounded configuration
reader below is wired to `ySIGetTrait "filename"` while preserving the existing
in-memory `ySIGetTrait` / `ySISetTrait` `"tag"` and `"icon"` behavior.

## Source mapping

The bounded `ySIGetTrait "filename" item` / `item.ySIGetTrait "filename"` mapping
evaluates the documented configuration fields through existing record and inventory
consumers. The explicit-item `"filename"` return is [observed: original FNV 1.4.0.525
with xNVSE 6.4.9 and yUI 140, single configured item]. Placed-item receiver forms,
multi-rule precedence/ties, and bad-item/no-rule outputs on `ySIGetTrait "filename"`
remain [hypothesis] until an interactive-desktop multi-rule capture runs.

- Explicit `mod` + hexadecimal local `form`, including a list of form strings,
  `formlist` (`0` direct form match, `1` `FLST` membership), and `formType`
  scalar/list predicates are evaluated [documented: field names, package author
  documentation above]. [hypothesis] `form` is the hexadecimal local FormID in
  the named plugin, `formlist: 1` tests membership in the referenced `FLST`
  (reading live `FLST` contents including runtime `AddFormToFormList` changes;
  other `formlist` modes such as `2` or non-`FLST` targets remain unknown), and
  `formType` uses the full engine form-type code table
  (`fnv_script::vm::form_type_code`). Placed references resolve to their base,
  and an existing temporary item base resolves through its original [engine
  design choice].
- The package's `questItem` condition reads the existing World quest-item predicate.
  [hypothesis] Integer 0/1 mean false/true; other representations remain unknown. The
  predicate includes the authored quest-item flag and saved SetQuestObject overrides,
  including an explicit clear of an authored flag. [engine design choice] Rules are
  cached, while this predicate reads current World state on each query; native
  classification timing after a flag edit is unobserved.
- Record-backed `WEAP`, `ALCH`, and `ARMO` predicates are evaluated against existing
  `Game` record/override views (`schemas/fnv/objects.json`, `nvse.rs`, `nvse_jip7.rs`,
  `nvse_jip8.rs`). Every supported key name below appears verbatim in the public
  **Sorting Icons 1.4b** (`ySI - Sorting Ycons`, Nexus mod 74358) `menus/ySI/ySI.json`
  configuration file; no ySI/yUI DLL or source code was consulted. Any key not found in
  a cited public source (including shorthand typos such as `weaponAutomatic` or
  `weaponScope`) remains `Match::Unknown`:
  - `WEAP` (`Match::No` for non-weapons [hypothesis]):
    - `weaponSkill` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json`;
      `schemas/fnv/objects.json` `WEAP.DNAM.skill`, actor-value skill IDs `34`/`35`/`38`/`41`/`45`;
      array support via `number_set`: hypothesis].
    - `weaponType` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json`, scalar and
      integer arrays such as `[3, 4]`, `[5, 6, 7]`, `[8, 9]`, `[11, 12]`;
      `schemas/fnv/objects.json` `WEAP.DNAM.animation_type`].
    - `weaponIsAutomatic` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used
      with `1`); `schemas/fnv/objects.json` `WEAP.DNAM.flags1` bit 1 `"Is Automatic"`].
    - `weaponHasScope` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with
      `1`); `schemas/fnv/objects.json` `WEAP.DNAM.flags1` bit 2 `"Has Scope"`].
    - `weaponIgnoresDTDR` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used
      with `1`); `schemas/fnv/objects.json` `WEAP.DNAM.flags1` bit 0 `"Ignores Normal
      Weapon Resistance"`].
  - `ALCH` (`Match::No` for non-ingestibles [hypothesis]):
    - `aidIsFood` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with `1`);
      `schemas/fnv/objects.json` `ALCH.ENIT.flags` bit 1 (`0x02`) `"Food Item"`].
    - `aidIsMedicine` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with
      `1`); `schemas/fnv/objects.json` `ALCH.ENIT.flags` bit 2 (`0x04`) `"Medicine"`].
    - `aidIsAddictive` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with
      `1`); `schemas/fnv/objects.json` `ALCH.ENIT.withdrawal_effect`; hypothesis:
      evaluated as non-null withdrawal form].
  - `ARMO` (`Match::No` for non-armor [hypothesis]):
    - `armorClass` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with `1`,
      `2`, `3`); GECK wiki `GetArmorClass` (`1` Light, `2` Medium, `3` Heavy) via
      `Game::armor_class`; array support via `number_set`: hypothesis].
    - `armorPower` [documented: Sorting Icons 1.4b `menus/ySI/ySI.json` (used with `1`);
      `schemas/fnv/objects.json` `ARMO.BMDT.general_flags` bit 5 `"Power Armor"` / GECK
      wiki `IsPowerArmor`].
    - The 11 `BMDT/biped_flags` slot booleans, each individually [documented: Sorting
      Icons 1.4b `menus/ySI/ySI.json`; `schemas/fnv/objects.json` `ARMO.BMDT.biped_flags`]:
      `armorHead` (bit 0 `"Head"`), `armorHair` (bit 1 `"Hair"`), `armorUpperBody`
      (bit 2 `"Upper Body"`), `armorPipBoy` (bit 6 `"PipBoy"`), `armorBackpack`
      (bit 7 `"Backpack"`), `armorHeadband` (bit 9 `"Headband"`), `armorHat`
      (bit 10 `"Hat"`), `armorEyeglasses` (bit 11 `"Eye Glasses"`), `armorMask`
      (bit 14 `"Mask"`), `armorChoker` (bit 15 `"Choker"`), and `armorMouthObject`
      (bit 16 `"Mouth Object"`).
  - Boolean value encoding: `1` matches true and `-1` matches false [documented: Sorting
    Icons 1.4b `menus/ySI/ySI.json` uses `1` and `-1` on armor-slot rules]; accepting `0`
    as false and `-1` on weapon/aid/power flags is [hypothesis]. Any other JSON value
    remains `Match::Unknown`.
- [engine design choice] Other predicates, including `formlist: 2`, threshold/ammo/misc
  predicates (`armorDT`, `weaponClipRounds`, `weaponNumProjectiles`, `weaponAttackAnim`,
  `weaponHandgrip`, `weaponSoundLevel`, `aidRestoresAV`, `aidDamagesAV`, `aidIsWater`,
  `aidIsPoisonous`, `miscComponent`, `miscProduct`, `ammoMod`, `ammoForm`), remain
  **unknown**, rather than false.
  A certainly false supported predicate rejects a rule even if it also has unknown
  predicates. A possible unknown rule at or above the winning priority refuses the
  query; an unknown lower rule cannot win.
- Multiple loose configuration files contribute rules [documented: package author
  documentation above]. [engine design choice] Empty or comment-only auxiliary
  `.json` files are treated as empty rule sets while still counting toward the file
  and byte bounds; the main `menus/ySI/ySI.json` must still be a valid JSON object.
  Equal-priority distinct tags refuse instead of guessing native file order.
  Identical winning tag outputs agree. Winning icons must have one unambiguous
  highest-priority filename. Missing tags, missing filenames and conflicting icon
  ties refuse on `"filename"`; no native default is invented.
- [engine design choice] The main `menus/ySI/ySI.json` is required. The existing Data
  reader selects each loose file/engine overlay, with the existing host reader as
  fallback for the main file. Additional archive-only JSON enumeration is not
  supported. A private local reader accepts comments and trailing commas, preserving
  quoted content [hypothesis: that the original reader accepts them too]. Generic JSON
  functions and their strict failure behavior are unchanged.
- [engine design choice] Configuration is a snapshot at the first query in each Game
  session, including a load failure. Restore creates a fresh cache. This timing avoids
  repeated filesystem/rule parsing during inventory comparisons; it does not establish
  native hot-reload timing or recover after late file attachment in the same session.
- [engine design choice] Accepted configuration is capped at 128 files, 4 MiB total
  parsed input and 20,000 rules (resource bounds, not native limits; raised from 32 files
  so heavily patched load orders with 50+ loose `menus/ySI/*.json` files do not fail the
  file-count cap). The existing reader obtains a file before its accepted-size check; this
  is not a pre-read allocation cap.

`ysi_filename` handles the `"filename"` trait on `ySIGetTrait` (case-insensitive)
with strict refusal: missing/default outputs, unsupported potentially winning
predicates, conflicting priority/icon ties, invalid/empty receivers, and non-item
forms explicitly refuse rather than inventing native defaults. The same cached
configuration snapshot also backs `crates/fnv-game/src/nvse_small.rs` for
`ySIGetTrait` / `ySISetTrait` `"tag"` and `"icon"` [documented: GECK wiki
`YSIGetTrait`/`YSISetTrait`, `"tag"` returns the sorting tag and `"icon"` returns
the sorting icon texture path; changing a tagged form's icon applies to every form
sharing that tag]. On `"tag"` and `"icon"`, session overrides set by `ySISetTrait`
take precedence over JSON rules (including explicit `""` tag clearing), non-item
forms are accepted [hypothesis: observed in community scripts querying world
references and classes], and unavailable/unmatched/blocked configuration returns
`""` rather than raising a script error [engine design choice]. Unhandled traits
such as `"string"` continue to return `""`. No `yUI` provider presence/version is
advertised (`ext-version.md`). This does **not** repair JAM sorting or establish
full ySI/JAM compatibility. A configuration with unresolved potentially winning
predicates cannot claim a filename merely because a lower supported rule exists.
No original JSON, icons, scripts or private paths are included.

## Source string arguments

[documented] Old string-parameter functions require explicit string conversion or
an inline expression when given a string variable
([String Variable](https://geckwiki.com/index.php?title=String_Variable),
[Inline Expression](https://geckwiki.com/index.php?title=Inline_Expression)).
The source-to-UI controls exercise `SetUIString "path" (icon)`, `SetUIStringAlt ("path") $icon`,
and the legacy bare-token convention `SetUIString "path" icon` with independently
authored paths and values. Compiler, VM and UI string-argument behavior are unchanged.

## Authored controls

Sixteen source controls live in the dedicated `ysi_filename_source` test target
(`crates/fnv-game/tests/ysi_filename_source.rs`), isolated because the existing
file-reader roots are process-wide. They use independently authored configuration,
items, and menu XML to cover:

1. Explicit item, array-element, and placed-reference receivers feeding `SetUIString`
   / `SetUIStringAlt` UI image filename traits without mutating inventory or world state.
2. Multi-file priority resolution and explicit refusal on equal-priority distinct-tag ties.
3. Unknown higher-priority predicates refusing, while lower or decisively-false
   rules with unknown predicates do not block a certain winner.
4. Invalid/empty receivers, non-item forms, and missing icons refusing without
   side effects, while preserving existing in-memory `ySISetTrait`/`ySIGetTrait`
   `"tag"` and `"icon"` behavior (also covered by `small_libs`).
5. `Game` save/restore snapshot lifetime (session cache stays stable until restore,
   which creates a fresh cache while retaining inventory).
6. Local commented/trailing-comma JSON parsing preserving quoted content while
   leaving generic `ReadFromJSON` strict.
7. File-count cap (128 files) and malformed/unterminated comment refusal.
8. Authored, script-set, and script-cleared `questItem` flags through source and save.
9. `questItem` false-rule disqualification vs possible/malformed `questItem` refusal.
10. Aggregate 4 MiB accepted-byte bound (exact limit accepted across two files;
    +1 byte refused without UI/world side effects; retained error snapshot vs fresh `Game`).
11. Aggregate 20,000 valid-rule bound (exact limit reaches final winner including
    icon rule; +1 valid rule refused without UI/world side effects).
12. JSON-backed `ySIGetTrait "tag"` / `"icon"` fallback, `ySISetTrait` override
    precedence, cross-form tag-icon sharing across JSON-tagged forms, tag
    reassignment switching JSON icons, explicit `""` tag clearing, and missing-config
    `""` fallback.
13. Full `formType` codes (`IMOD` 103, `ACTI` 21) and non-item `"tag"`/`"icon"`
    queries while keeping `"filename"` restricted to live inventory items.
14. `"formlist": 0 | 1` authored and live `AddFormToFormList` membership alongside
    conservative `Match::Unknown` refusal for `"formlist": 2` and non-`FLST` targets.
15. Empty/comment-only auxiliary `.json` stub files ignored cleanly while empty
    `ySI.json` and unterminated auxiliary comments continue to refuse explicitly.
16. Record-backed `WEAP`, `ALCH`, and `ARMO` predicates (`weaponSkill`, `weaponType`,
    `weaponIsAutomatic`, `weaponHasScope`, `weaponIgnoresDTDR`, `aidIsFood`,
    `aidIsMedicine`, `aidIsAddictive`, `armorClass`, `armorPower`, and `BMDT/biped_flags`
    slot booleans with `1` / `0` / `-1`), cross-type `Match::No` disqualification, and
    conservative `Match::Unknown` refusal for malformed boolean values.

These are source and UI-trait consumer checks, not texture decoding, GPU rendering,
multi-rule native parity, or complete installed ySI/JAM acceptance.

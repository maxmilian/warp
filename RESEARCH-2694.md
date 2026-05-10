# Issue #2694 Investigation — `python <TAB>` file completion broken in Warp

**Issue:** https://github.com/warpdotdev/warp/issues/2694
**Status:** Root-cause investigation (not implemented yet)
**Date:** 2026-05-10 (updated with verification round 2)
**Branch:** `claude/python-completion-research-33n7s`

## TL;DR (verified)

When `python <TAB>` (or any unknown command) is pressed:

1. Warp's input editor intercepts `Tab` in Rust at `app/src/terminal/input.rs:10057` → `input_tab()` → builds a completion request with `CompletionsTrigger::Keybinding`.
2. The fallback strategy is computed at `app/src/terminal/input.rs:11253-11258`:
   ```rust
   let fallback_strategy = match completions_trigger {
       CompletionsTrigger::Keybinding if !use_native_shell_completions => {
           CompletionsFallbackStrategy::FilePaths
       }
       _ => CompletionsFallbackStrategy::None,
   };
   ```
   When `use_native_shell_completions == true` (zsh + flag/pref), fallback becomes `None`.
3. The engine looks up `python` in the spec registry (`crates/warp_completer/src/signatures/v2/lookup.rs:50`). **No spec exists** for `python`/`python3`/`pip`/`pytest` anywhere in `crates/warp_completer/` or `crates/command-signatures-v2/`. Lookup returns `None`.
4. Because spec is missing AND `fallback_strategy != FilePaths`, the engine emits zero suggestions (`crates/warp_completer/src/completer/engine/argument/v2.rs:92-99`).
5. In parallel, Warp sends `0x19` (^Y) to the PTY (`app/src/terminal/writeable_pty/pty_controller.rs:699`), which fires `warp_complete_via_compadd_override` in zsh. zsh runs `_python` which calls `_files -g '*.py'`. `compadd` is shadowed by Warp's override at `app/assets/bundled/bootstrap/zsh_body.sh:1193`. If anything matches in cwd, results are emitted via OSC `9280;C`. If nothing matches, the override returns silently at line 1239 — but `9280;A` and `9280;B` markers still bracket the (empty) emission, so Rust correctly receives an empty native result vector.
6. Merge at `app/src/terminal/input.rs:11319-11343`: engine empty → fall through to native; native empty → final result is `None`. **No fallback path exists at this point** to invoke `FilePaths` because the strategy was already baked in upstream.

User sees: nothing.

## Verified file/line map

| File | Line | What | Notes |
|---|---|---|---|
| `app/src/terminal/input.rs` | 10057 | `EditorEvent::Navigate(NavigationKey::Tab)` handler | Tab is fully intercepted in Rust |
| `app/src/terminal/input.rs` | 12002 | `input_tab(ctx)` entry | Builds completion request |
| `app/src/terminal/input.rs` | 11237-11243 | reads `ForceNativeShellCompletions` user pref | Private pref, no public UI |
| `app/src/terminal/input.rs` | 11245-11251 | computes `use_native_shell_completions` | flag OR pref AND zsh AND single-line |
| `app/src/terminal/input.rs` | 11253-11258 | sets `fallback_strategy` — **root cause line** | `None` when native enabled |
| `app/src/terminal/input.rs` | 11285-11298 | dispatches `RunNativeShellCompletions` action | Sends ^Y to PTY |
| `app/src/terminal/input.rs` | 11319-11343 | merge engine + native results | No FilePaths fallback path here |
| `app/src/terminal/input.rs` | 2045-2048 | `enum CompletionsTrigger { Keybinding, AsYouType }` | Both routes go to `None` when native is on |
| `app/src/terminal/view.rs` | 25564-25568 | handles `RunNativeShellCompletions` | Forwards to PTY controller |
| `app/src/terminal/writeable_pty/pty_controller.rs` | 699 | sends `0x19` (^Y) byte to PTY | The actual trigger |
| `app/src/terminal/writeable_pty/pty_controller.rs` | 109,175,189 | `in_flight_native_completions_state` | Lifecycle & results channel |
| `app/src/terminal/model/ansi/mod.rs` | 86 | `WARP_COMPLETIONS_OSC_MARKER` const (`9280`) | OSC parser entry |
| `app/src/terminal/model/ansi/mod.rs` | 1140-1212 | parses `9280;A/B/C/D` into `ShellCompletion` | Ingest path |
| `app/src/terminal/model/terminal_model.rs` | 3241-3256 | accumulates `ShellCompletion`s, emits `CompletionsFinished` | Empty vec → empty result |
| `crates/warp_completer/src/completer/suggest/mod.rs` | 480-483 | `enum CompletionsFallbackStrategy { FilePaths, None }` | Two-variant enum |
| `crates/warp_completer/src/completer/suggest/mod.rs` | 507 | default strategy = `FilePaths` (in completer-internal default) | Overridden by caller |
| `crates/warp_completer/src/completer/engine/argument/v2.rs` | 92-99 | empty + no spec + `FilePaths` → `sorted_paths_relative_to()` | This is what we want triggered |
| `crates/warp_completer/src/completer/engine/argument/v2.rs` | 471-484 | empty argument values fallback (same gate) | |
| `crates/warp_completer/src/completer/engine/argument/legacy.rs` | 104, 549 | legacy version of the same gate | |
| `crates/warp_completer/src/signatures/v2/lookup.rs` | 50 | `get_matching_signature_for_tokenized_input` | Returns `None` for unknown commands |
| `crates/warp_terminal/src/shell/mod.rs` | 376-378 | `supports_native_shell_completions` → `matches!(self, ShellType::Zsh)` | **Only zsh** |
| `crates/warp_features/src/lib.rs` | 183 | `NativeShellCompletions` flag declared | Default OFF (not in DOGFOOD/PREVIEW/RELEASE lists) |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1193-1257 | `compadd` override | Recently fixed in PR #10535 |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1239 | `[[ -n $__hits ]] || return` | Silent early-out on empty |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1254-1256 | emits `\e]9280;C…\e\\` per match | Emission protocol |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1262, 1275 | `9280;A` start / `9280;B` end markers | Bracket every completion run |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1308-1318 | `warp_complete_via_compadd_override` | Sets `COMPADD_OVERRIDE=true` |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1357-1359 | `bindkey '^Y'` (compadd path) and `'^X'` (list-choices path) | No `^I` (Tab) binding |

## Verified answers to original open questions

1. **Does `python` have an engine spec?** **No.** Confirmed empty for `python`, `python3`, `pip`, `pytest`. Lookup returns `None` → engine emits no suggestions for argument tokens.
2. **What triggers Tab → completion?** Tab is fully intercepted by Rust (`input.rs:10057`); zsh **never sees Tab**. Rust either calls the engine + (optionally) sends `^Y` to PTY for native zsh completions. `^I` is **not** bound in `zsh_body.sh`.
3. **Repro path is (a)+(b) combined:** Tab → Rust intercept → engine has no spec for `python` AND fallback strategy was set to `None` because native is on → engine returns empty → ^Y to zsh → `_python` runs but result still has to round-trip through Warp's `compadd` override and OSC pipeline → if cwd has no `.py` files, native is empty → merge returns empty → user sees nothing. The `fallback=None` is the master switch that locks out the safety net.
4. **`_python` on macOS zsh:** present in standard distro at `/usr/share/zsh/<version>/functions/Completion/Unix/Command/_python`; calls `_arguments` then `_files -g '*.py'`. Bug ALSO reproduces in directories without `.py` files even though native zsh would still offer arbitrary files via `_default`/`_files` fallback — but Warp's `_python` path likely runs `_files -g '*.py'` only.
5. **Feature flag:** `NativeShellCompletions` is **OFF by default** (`crates/warp_features/src/lib.rs:183`, not in `DOGFOOD_FLAGS`/`PREVIEW_FLAGS`/`RELEASE_FLAGS`). However the user-facing private preference `ForceNativeShellCompletions` (`input.rs:11237-11243`) ORs into the same boolean. **Caveat:** without native enabled, the `Keybinding` branch DOES set `FilePaths` and file completion works for `python <TAB>`. Issue #2694's reporter most likely has native completions enabled (intentionally or via flag rollout).

## Refined hypothesis

The bug fires only when `use_native_shell_completions == true`. Two independent failures combine:

- **A. Engine has no `python` spec** → engine returns empty.
- **B. `fallback_strategy = None` in native mode** → engine had no chance to emit `_files` results.
- **C. Native zsh path** runs but `_python` is a "smart" completer that filters to `*.py`, so in directories without `.py` files (or when the user wants to pass any file), it returns empty.

Result: silent empty completion menu.

When native is OFF (default for non-flagged users), `Keybinding` triggers `FilePaths` fallback in the engine and the user sees files. This explains why the bug is intermittent across reporters — it tracks whoever has flipped on native completions.

## Open questions still worth checking before fixing

- **Behavior with subdir `.py` files only:** does Warp's native path even offer file completions for `python file<TAB>` when there is at least one `.py` in cwd? Bench manually.
- **Does `force_native_shell_completions` have any persistence path beyond private prefs?** If a settings UI toggle exists in main, the test plan needs both states.
- **AsYouType regression risk:** Fix #1 needs to be careful — making AsYouType also fall back to FilePaths could spam completions on every keystroke. Must keep AsYouType on `None`.

## Candidate fixes (ranked, NOT implemented)

### Fix #1 (recommended) — Engine fallback runs even when native is on, for Keybinding only

**File:** `app/src/terminal/input.rs:11253-11258`

```rust
let fallback_strategy = match completions_trigger {
    // Keybinding: always offer file paths as last resort, regardless of native.
    CompletionsTrigger::Keybinding => CompletionsFallbackStrategy::FilePaths,
    // AsYouType: keep current behavior (no fallback) to avoid spammy file lists per keystroke.
    CompletionsTrigger::AsYouType => CompletionsFallbackStrategy::None,
};
```

Then in the merge logic at `input.rs:11319-11343`: if engine returned empty AND native returned empty AND `fallback_strategy == FilePaths`, do the file-paths fallback at the merge layer too. Alternatively, since the engine already runs `FilePaths` fallback when its lookup misses, this fix may be sufficient on its own — engine will emit file paths, merge will see non-empty engine results and use them.

**Risk:** Low. AsYouType behavior preserved. When native succeeds with non-empty, it still wins (engine results from FilePaths fallback would only appear when engine has no spec; if engine has spec it returns spec-driven results; merge prefers engine non-empty over native).

**Edge case:** When engine returns FilePaths fallback (e.g. for `python` arg) AND native ALSO returns results (e.g. zsh `_python` returns the same `*.py` files), the merge prefers engine. We'd lose zsh's `*.py` filtering quality. To preserve native-quality results when both succeed, prefer native when engine result is *only* the FilePaths fallback. Cleaner: pass an `is_fallback` bit through `SuggestionResults`.

### Fix #2 — Bootstrap signals empty explicitly so Rust can run fallback

**File:** `app/assets/bundled/bootstrap/zsh_body.sh:1239`

Replace silent `return` with `print -n "\e]9280;C;__WARP_EMPTY__\e\\"` (or a new sub-marker like `9280;E`). Add Rust parsing in `app/src/terminal/model/ansi/mod.rs` to track empty-with-confidence. Higher implementation cost, more brittle. **Not recommended** when Fix #1 already resolves the user-facing symptom.

### Fix #3 — Universal `_files` zsh fallback

Configure zsh in bootstrap to register `_files` as catch-all. Risky — alters every command's completion behavior, may regress commands that intentionally return empty.

### Fix #4 — Add a `python` spec

Adds value but doesn't address the architectural gap (any unknown binary still has the same bug). Could be done alongside Fix #1.

## Branch / PR plan (when ready to implement)

- Branch: `fix/2694-python-tab-completion-fallback` (off `claude/python-completion-research-33n7s`)
- Approach: Fix #1 only. Add a `is_fallback` boolean on engine results so merge can prefer non-fallback native over fallback engine when both non-empty.
- Tests:
  - `crates/warp_completer` argument fallback unit test: assert that with `FilePaths` strategy and unknown command, FilePaths fire; with `None`, they don't.
  - `crates/warp_completer` test: with `force_native_shell_completions` simulated true and `Keybinding` trigger, fallback strategy passed to engine is `FilePaths`.
  - Manual against zsh:
    - `python <TAB>` in dir with `.py` files → completes those files
    - `python <TAB>` in dir without `.py` files → completes any file (engine FilePaths fallback)
    - `pytest <TAB>` → same
    - `./mybin <TAB>` (no spec, no zsh `_mybin`) → completes any file
    - `cd <TAB>` (has spec) → unchanged, dirs only
    - As-you-type (`python pa<...>`) → does NOT spam file list (AsYouType keeps `None`)
- Reference PRs:
  - #10535 — compadd override fix for nested paths (already merged) — uses similar test pattern
  - #9711 — user's prior IME PR

## References

- Issue: https://github.com/warpdotdev/warp/issues/2694
- Sibling issue: https://github.com/warpdotdev/warp/issues/2677
- Related merged: #10535 (`compadd` override / `$IPREFIX` for nested paths)
- User's prior Warp PRs: #9711 (merged), #10535 / #10584 / #10586 (open)

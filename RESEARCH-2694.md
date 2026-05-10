# Issue #2694 Investigation — `python <TAB>` file completion broken in Warp

**Issue:** https://github.com/warpdotdev/warp/issues/2694
**Status:** Root-cause investigation (not implemented yet)
**Date:** 2026-05-10

## TL;DR

Native zsh completes `python file<TAB>` to `.py` files via `_python` → `_files`. Warp breaks this. Two-stage failure suspected:

1. **Rust ingest:** `app/src/terminal/input.rs:11253-11258` sets `CompletionsFallbackStrategy::None` whenever `use_native_shell_completions` is true → engine never falls back to file paths for unknown commands.
2. **zsh emit (likely innocent for `python`):** `app/assets/bundled/bootstrap/zsh_body.sh:1239` early-returns when `$__hits` is empty in the `compadd` override, but `_files` *should* call `compadd` so this shouldn't matter for `python` specifically.

The `None` fallback being intentional is **inferred, not proven** — only one squashed commit `fc1d2ff` (initial public release) blames here, no design doc found.

## Key files / lines

| File | Line | What |
|---|---|---|
| `app/src/terminal/input.rs` | 11253-11258 | `fallback_strategy = None` when native completions enabled |
| `app/src/terminal/input.rs` | 11320 | merge of engine + native results; if both empty → silent fail |
| `crates/warp_completer/src/completer/engine/argument/v2.rs` | 92-99 | `FilePaths` fallback gated on `fallback_strategy == FilePaths` |
| `crates/warp_completer/src/signatures/v2/lookup.rs` | 50 | spec lookup; returns `None` for `python` |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1193-1257 | `compadd` override (recently fixed in PR #10535) |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1239 | `[[ -n $__hits ]] || return` — drops empty results silently |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1308-1318 | `warp_complete_via_compadd_override` — sets `COMPADD_OVERRIDE=true` |
| `app/assets/bundled/bootstrap/zsh_body.sh` | 1357-1359 | bound to `^Y` (not Tab); `^X` runs `list-choices` path |

## Open verification questions (next steps)

1. **Does `python` actually have an engine spec?** Initial grep found nothing under `app/specs` or `crates/warp_completer/src/signatures`, but I didn't fully confirm.
2. **What keybinding actually triggers the override?** `compadd` override is bound to `^Y`, `list-choices` to `^X`. Need to find what `Tab` (`^I`) is bound to in Warp's input flow — possibly handled in Rust before reaching zsh.
3. **Repro path:** is the bug
   (a) tab handled in Rust → engine has no spec → no fallback → empty,
   (b) tab → zsh `_python` produces nothing → bootstrap returns empty → empty,
   (c) tab → zsh works fine but Rust ingest filters it out?
4. **Is `_python` shipped on standard macOS zsh?** `ls /usr/share/zsh/*/functions/Completion/Unix/Command/_python*` — if absent, zsh falls back to `_default` which uses `_files`. Should still work, so why doesn't Warp see it?
5. **Feature flag:** `NativeShellCompletions` and `ForceNativeShellCompletions` — which is on by default? Test with both on/off.

## Candidate fixes (ranked, NOT implemented)

### Fix #1 (recommended) — restore file-path fallback under native mode
**File:** `app/src/terminal/input.rs:11253-11258`

Change `fallback_strategy` so that even with native completions on, `FilePaths` is selected for `Keybinding` triggers. Use it only when both engine and native return empty.

```rust
let fallback_strategy = match completions_trigger {
    CompletionsTrigger::Keybinding => CompletionsFallbackStrategy::FilePaths,
    _ => CompletionsFallbackStrategy::None,
};
```

Then in the merge logic (~line 11320), if engine returned empty *and* native returned empty, fire the `FilePaths` fallback.

**Risk:** low — only adds completions when none would otherwise appear.

### Fix #2 — bootstrap-side: signal "I had nothing" instead of silent return
**File:** `app/assets/bundled/bootstrap/zsh_body.sh:1239`

Replace silent `return` with an OSC marker like `9280;C;EMPTY` so Rust knows native produced nothing and can trigger fallback. Higher complexity, requires Rust-side parsing changes.

### Fix #3 — make zsh always offer `_files` for unknown commands
Configure zsh in bootstrap to add `_files` as the universal fallback completer. Risky — may produce unexpected completions for many commands.

## Branch / PR plan (when ready to implement)

- Branch: `fix/2694-python-tab-completion-fallback`
- Approach: start with Fix #1 (smallest blast radius)
- Tests:
  - `crates/warp_completer` filter-contract test mirroring #10535's pattern
  - Manual: `python <TAB>`, `pytest <TAB>`, `./mybin <TAB>` (any unknown command) should file-complete
- Linked PRs to learn from: #10535 (compadd override fix), #9711 (IME)

## References

- Sibling issue: https://github.com/warpdotdev/warp/issues/2677
- Related (already merged): #10535 — fixed `compadd` override losing `$IPREFIX` for nested paths
- User's prior Warp PRs: #9711 merged; #10535/#10584/#10586 open

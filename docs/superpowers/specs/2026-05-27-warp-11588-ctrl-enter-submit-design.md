# warp #11588 — Rich Input: Submit on Ctrl+Enter toggle

**Issue:** [warpdotdev/warp#11588](https://github.com/warpdotdev/warp/issues/11588)
**Date:** 2026-05-27
**Author:** Max Hsu (@maxmilian)
**Status:** Design approved

## Summary

Add a user-facing toggle in **Settings → Agents → Third party CLI agents** that
switches the CLI-agent Rich Input editor's submit binding. Default behavior is
unchanged (Enter submits). When the toggle is on:

- `Enter` inserts a newline into the Rich Input buffer.
- `Ctrl+Enter` submits the Rich Input to the active CLI agent.

`Ctrl+Enter` is cross-platform (no macOS `Cmd+Enter` variant), matching the issue
title and the Slack / Discord / Cursor / Copilot Chat convention. `Cmd+Enter` is
already used by `input_cmd_enter` for NLD / Agent View entry and is intentionally
left alone.

Scope is strictly the CLI-agent Rich Input editor (the input UI shown over a
running `claude` / `codex` / `gemini` / Copilot / OpenCode / Auggie / CursorCli
session). The regular terminal shell input, the Warp Agent AI query input, and
all other editors are unaffected.

## Motivation

Multi-line composition is awkward when `Enter` always submits. Users drafting
longer prompts to a CLI agent expect the IDE-style `Ctrl+Enter` submit
convention. Today there is no way to get newline insertion at all in the Rich
Input without leaving the agent session.

## Non-Goals

- Generalized keybinding remapping UI.
- Changing the submit binding for non-Rich-Input editors (terminal shell input,
  command palette, search bar, AI query input, code editor, etc.).
- Per-agent override of the submit binding.
- `Cmd+Enter`-on-macOS variant.
- Changing what `Shift+Enter` or `Alt+Enter` currently do in the Rich Input.

## Background — relevant existing code

- `app/src/editor/view/mod.rs:5423-5478` — editor framework already emits
  `EditorEvent::Enter` / `CmdEnter` / `CtrlEnter` / `ShiftEnter` / `AltEnter`,
  and exposes per-modifier `enter_settings` with
  `EnterAction::InsertNewLineIfMultiLine`. `newline_internal` is a stable
  `editor_model.insert("\n", ...)` op.
- `app/src/terminal/input.rs:10235-10239` — translates editor events into
  `input_enter` / `input_cmd_enter` / `Event::CtrlEnter` (no current handler for
  Ctrl+Enter beyond a passive event emit).
- `app/src/terminal/input.rs:12459-12515` — `input_enter` body. The Rich Input
  branch is the `if CLIAgentSessionsModel::as_ref(ctx).is_input_open(view_id)`
  block; it handles AI context menu / prompts menu / skill menu / slash commands
  menu fallthroughs and finally emits `Event::SubmitCLIAgentInput { text }`.
- `app/src/terminal/view.rs:21110-21112` — receives `SubmitCLIAgentInput` and
  calls `submit_cli_agent_rich_input(text, ctx)`.
- `app/src/terminal/view.rs:20802-20811` — existing consumer of
  `InputEvent::CtrlEnter` for "accept prompt suggestion" (Warp Agent feature,
  not Rich Input). Must not regress.
- `app/src/settings/ai.rs:711+` — `AISettings` `define_settings_group!`. Other
  Rich Input toggles live here (`auto_dismiss_rich_input_after_submit` at L1294,
  `auto_toggle_rich_input` at L1268, `auto_open_rich_input_on_cli_agent_start`
  at L1280). All three sit under the `agents.third_party.*` TOML namespace and
  use the legacy term `composer` in their TOML keys (the feature was renamed
  to "Rich Input" in the Rust field names but kept `composer` in TOML for
  backward-compat — e.g. `auto_dismiss_composer_after_submit`,
  `auto_toggle_composer`, `auto_open_composer_on_cli_agent_start`).
- `app/src/settings_view/ai_page.rs:6233-6440` — `CLIAgentWidget`. Pattern for
  adding a new toggle in the Rich Input sub-section is established by
  `ShouldRenderCLIAgentToolbar` (L6260) and the
  `auto_dismiss_rich_input_after_submit` block (L6370+).
- `app/src/features.rs` — `FeatureFlag::CLIAgentRichInput` gates the Rich Input
  feature. The new toggle is only shown / honored when this flag is enabled.

## Design

### 1. Settings schema

Add to `AISettings` in `app/src/settings/ai.rs` (alongside the other Rich Input
settings):

```rust
submit_rich_input_on_ctrl_enter: SubmitRichInputOnCtrlEnter {
    type: bool,
    default: false,
    supported_platforms: SupportedPlatforms::ALL,
    sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
    private: false,
    toml_path: "agents.third_party.submit_composer_on_ctrl_enter",
    description:
        "When enabled, the CLI agent Rich Input submits on Ctrl+Enter and \
         Enter inserts a newline instead of submitting.",
},
```

`default: false` preserves current behavior.

TOML key uses the legacy `composer` term to match the three sibling Rich Input
settings already under `agents.third_party.*` (`auto_dismiss_composer_after_submit`,
`auto_toggle_composer`, `auto_open_composer_on_cli_agent_start`). Rust field
name stays `submit_rich_input_on_ctrl_enter` to match user-facing terminology.

### 2. UI

In `app/src/settings_view/ai_page.rs`:

- Add to `AISettingsPageAction` enum (~L2612):
  ```rust
  ToggleSubmitRichInputOnCtrlEnter,
  ```
- Add handler arm (~L2916, alongside `ToggleCLIAgentToolbar`):
  ```rust
  AISettingsPageAction::ToggleSubmitRichInputOnCtrlEnter => {
      // toggle + telemetry, following the ToggleAutoToggleRichInput pattern
  }
  ```
- Add field to `CLIAgentWidget` struct (L6233):
  ```rust
  submit_rich_input_on_ctrl_enter_toggle: SwitchStateHandle,
  ```
- Add `search_terms` keywords: `"ctrl enter newline submit multiline rich input"`.
- Render the toggle inside the existing
  `if is_footer_enabled && FeatureFlag::CLIAgentRichInput.is_enabled()` block,
  positioned adjacent to `auto_dismiss_rich_input_after_submit`. Use the
  `render_body_item_label` + `build_toggle_element` + `render_ai_feature_switch`
  pattern that the surrounding toggles use.
- Label: `"Submit Rich Input with Ctrl+Enter (Enter inserts newline)"`.

### 3. Dispatch logic

In `app/src/terminal/input.rs`:

**a. Extract a helper from the existing `input_enter` Rich Input branch.**
The body from L12461 through `ctx.emit(Event::SubmitCLIAgentInput { text }); return;`
(L12513-12514) moves into:

```rust
/// Handles a submit press while the CLI-agent Rich Input is open. Returns
/// `true` if the event was fully handled (caller should return early).
fn handle_rich_input_submit(&mut self, ctx: &mut ViewContext<Self>) -> bool {
    // Existing 12461-12514 body verbatim — menu fallthroughs first, then
    // text extraction + `Event::SubmitCLIAgentInput` emit.
    // ... (no logic change)
    true
}
```

**b. Branch on the setting in `input_enter` (L12459).**
At the top of the Rich Input branch:

```rust
if CLIAgentSessionsModel::as_ref(ctx).is_input_open(self.terminal_view_id) {
    let submit_on_ctrl_enter =
        *AISettings::as_ref(ctx).submit_rich_input_on_ctrl_enter;

    // Menu fallthroughs always win (selecting a menu item is never a newline).
    if self.has_open_rich_input_menu(ctx) {
        return self.handle_rich_input_submit(ctx);
    }

    if submit_on_ctrl_enter {
        // Enter inserts a newline; Ctrl+Enter (handled below) submits.
        self.editor.update(ctx, |editor, ctx| editor.newline_internal(ctx));
        return;
    }

    self.handle_rich_input_submit(ctx);
    return;
}
```

`has_open_rich_input_menu` is a small helper that returns true when any of
{AI context menu, prompts menu, skill menu, slash commands menu} is open — it
collapses the four `is_*_menu()` checks already inlined in the current body.

**c. Branch on the setting in `EditorEvent::CtrlEnter` (L10237).**
Replace the current arm:

```rust
EditorEvent::CtrlEnter => {
    let submit_on_ctrl_enter =
        *AISettings::as_ref(ctx).submit_rich_input_on_ctrl_enter;
    let in_rich_input =
        CLIAgentSessionsModel::as_ref(ctx).is_input_open(self.terminal_view_id);

    if submit_on_ctrl_enter && in_rich_input {
        self.handle_rich_input_submit(ctx);
    } else {
        ctx.emit(Event::CtrlEnter);
    }
}
```

The existing `is_accept_prompt_suggestion_bound_to_ctrl_enter` consumer in
`view.rs:20802` continues to fire on `InputEvent::CtrlEnter` for non-Rich-Input
contexts. The new branch only short-circuits when both
`submit_on_ctrl_enter == true` AND we're in a Rich Input session — so the
prompt-suggestion-accept feature is not regressed.

### 4. `newline_internal` visibility

`newline_internal` at `app/src/editor/view/mod.rs:5445` is currently private.
Implementation will need to expose it (e.g. `pub(crate)`) — or, if cross-crate
visibility is awkward, add a thin public method `insert_newline_at_cursor`
that wraps it. Verify the minimal exposure during implementation.

### 5. Tests

Add to `app/src/terminal/input_tests.rs`:

1. **`rich_input_default_enter_submits`** — toggle OFF (default), open Rich
   Input, press Enter → `Event::SubmitCLIAgentInput` emitted, buffer cleared.
2. **`rich_input_default_ctrl_enter_does_not_submit`** — toggle OFF, Ctrl+Enter
   → no `SubmitCLIAgentInput` (existing `Event::CtrlEnter` emit untouched).
3. **`rich_input_ctrl_enter_mode_enter_inserts_newline`** — toggle ON, open
   Rich Input, type "foo", press Enter → buffer == "foo\n", no submit emitted.
4. **`rich_input_ctrl_enter_mode_ctrl_enter_submits`** — toggle ON, Ctrl+Enter
   → `SubmitCLIAgentInput { text }` emitted with current buffer.
5. **`rich_input_ctrl_enter_mode_menu_open_still_selects`** — toggle ON, open
   AI context menu / prompts menu, press Enter → menu item selected (not
   newline). Repeat for skill menu + slash commands menu (4 assertions in one
   test).
6. **`ctrl_enter_toggle_does_not_affect_shell_input`** — toggle ON, Rich Input
   NOT open, regular shell input + Enter → still runs the command path
   (existing behavior intact).

### 6. Telemetry

Follow the `ToggleCLIAgentToolbarSetting` pattern at `ai_page.rs:2924` — emit a
`TelemetryEvent::ToggleSubmitRichInputOnCtrlEnterSetting { enabled: bool }`
event when the toggle is flipped. Add the variant to the appropriate telemetry
enum.

### 7. PR body / visual evidence (per `feedback_warp_ui_visual_evidence`)

PR body must embed (Oz only greps the body, not comments):

- **Screenshot** of the new toggle in
  Settings → Agents → Third party CLI agents.
- **Recording** of the Rich Input editor showing:
  1. Toggle OFF → Enter submits (existing behavior).
  2. Toggle ON → Enter inserts newline; Ctrl+Enter submits.

Recording > static screenshot per the cached feedback.

## Risks / open questions

1. **`newline_internal` visibility** — needs `pub(crate)` exposure or a thin
   wrapper. Trivial but cross-crate.
2. **Settings sync namespace verified** — TOML namespace is
   `agents.third_party.*` and sibling keys use the legacy `composer` term
   (`auto_dismiss_composer_after_submit` etc.). Spec uses
   `agents.third_party.submit_composer_on_ctrl_enter`.
3. **Single-line vs multi-line editor** — Rich Input is configured as a
   multi-line editor; `newline_internal` works directly. If a Rich Input
   instance ever runs as `single_line` (unexpected), the newline insertion
   would be lost. Verify Rich Input editor construction passes
   `single_line = false`. Implementation step.
4. **Issue not yet `ready-to-implement` blocking review?** — Already labeled
   `ready-to-implement` + `repro:high` (verified 2026-05-27). Oz should review
   without the `feedback_oz_ready_to_implement` issue surfacing.

## Acceptance criteria (mirrors issue)

- [ ] `submit_rich_input_on_ctrl_enter` setting declared with `default: false`,
      `sync_to_cloud: Globally`.
- [ ] Toggle rendered in `CLIAgentWidget`, gated by `FeatureFlag::CLIAgentRichInput`,
      following surrounding toggle patterns.
- [ ] Rich Input Enter behavior switches as designed; other editors untouched.
- [ ] Setting persists across app restarts via standard sync.
- [ ] 6 unit tests pass; existing `input_tests.rs` and `view_tests.rs` Rich
      Input tests still pass.
- [ ] PR body includes screenshot of toggle + recording of Rich Input behavior
      change.
- [ ] `cargo check -p warp`, `cargo clippy -p warp_terminal`, `cargo fmt`,
      `cargo test -p warp_terminal` all green.

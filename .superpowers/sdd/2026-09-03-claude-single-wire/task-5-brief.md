### Task 5: Lean slice — system assembly, stream accumulation, witnesses, re-pointed fence

This task is deliberately allowed to leave one Rust test red: `generated_claude_body_cases_drive_the_body_builder` expects `Message::System` rows inside `system[]`, which Task 6 implements. Every other test is green at the end of this task. The Lean build is zero-`sorry`.

**Files:**
- Modify: `crates/gents/proofs/Proofs/PromptAssembly/ClaudeMap.lean` (append after `mappedKind_calls`, before `end PromptAssembly.ClaudeMap`; extend `MapError`/`errorName`)
- Modify: `crates/gents/proofs/Proofs/Conformance/ContractCases/PromptAssembly.lean:524-554` (`surfaceOf`, two new witness structures + case lists before `end Conformance.ContractCases`)
- Modify: `crates/gents/proofs/Proofs/Conformance/Contracts/Json/PromptAssembly.lean:110-123` (two renderers)
- Modify: `crates/gents/proofs/Proofs/Conformance/Contracts/Json/Snapshot.lean:247-248` (two keys)
- Modify: `crates/gents/proofs/Proofs/Conformance/CoverageLedger.lean:995-999` (rename consumer id; two new entries)
- Modify: `crates/gents/src/lean_vocab_test/prompt_assembly.rs:75-84` (two structs)
- Modify: `crates/gents/src/lean_vocab_test/support.rs:169-170` (two fields), `:1013-1016` (two accessors)
- Modify: `crates/gents/tests/conformance/coverage.rs:817-822` (two emitted-set blocks)
- Modify: `crates/gents/tests/support/conformance_consumers.rs:674-680` (rename; two registry entries)
- Modify: `crates/gents/tests/conformance/prompt_assembly.rs:27-31,325-422` (imports, re-pointed driver, two new drivers)
- Modify: `crates/gents/src/claude_messages.rs` (`parse_messages_sse` and `build_messages_body` become `pub` if not already; add `pub fn claude_code_identity() -> &'static str` is unnecessary — `CLAUDE_CODE_IDENTITY` is already `pub`)

**Interfaces:**
- Consumes: `claude_messages::parse_messages_sse(&str, &HashSet<String>) -> Result<Vec<RawStreamingChoice<ClaudeStreamResponse>>, CompletionError>`; `claude_messages::build_messages_body(&str, &CompletionRequest) -> Value`; `claude_messages::CLAUDE_CODE_IDENTITY`; error Display strings from Task 1.
- Produces (snapshot keys and Rust types used by Task 6's green check):
  - `prompt_assembly_claude_body_cases: Vec<LeanPromptAssemblyClaudeBodyCase { name, preamble: Option<String>, rows: Vec<String>, tools: Vec<String>, system: Vec<String>, tools_present: bool }>`
  - `prompt_assembly_claude_stream_cases: Vec<LeanPromptAssemblyClaudeStreamCase { name, surface: Vec<String>, events: Vec<String>, outcome: String, calls: Vec<String> }>` where `events` tags are `text:<t>`, `start:<id>:<name>:<input-or-empty>`, `delta:<partial>`, `stop`; `calls` entries are `<id>=<argsJson>`.
  - Lean names: `ClaudeMap.identity`, `systemBlocks`, `splitSystem`, `toolsField`, `StreamEvent`, `accumulate`, `runStream`, `MapError.overlappingBlock`.

- [ ] **Step 1: Extend `MapError` in `ClaudeMap.lean`**

Replace the `MapError` inductive and `errorName`:

```lean
inductive MapError where
  | emptySurface
  | unmappedName (name : String)
  | duplicateId (id : ToolCallId)
  | overlappingBlock (id : ToolCallId)
  deriving DecidableEq, Repr

def errorName : MapError → String
  | .emptySurface => "emptySurface"
  | .unmappedName name => "unmappedName:" ++ name
  | .duplicateId id => "duplicateId:" ++ toString id
  | .overlappingBlock id => "overlappingBlock:" ++ toString id
```

- [ ] **Step 2: Append the system-assembly model to `ClaudeMap.lean`** (before `end PromptAssembly.ClaudeMap`)

```lean
/-! ## System assembly (single-wire Messages HTTP)

`Transcript.MessageRole` has no `system`; the wire-side row type here carries
just what assembly needs: a `System` row's text, or "some other row". -/

inductive Msg where
  | system (text : String)
  | other (tag : String)
  deriving DecidableEq, Repr

/-- The Claude Code identity block. `system[0]` on every request; the seat's
oat routes on it. Checked against Rust `CLAUDE_CODE_IDENTITY` by the vocab test. -/
def identity : String := "You are Claude Code, Anthropic's official CLI for Claude."

/-- Pull `System` rows out in order; everything else is untouched. -/
def splitSystem : List Msg → List String × List Msg
  | [] => ([], [])
  | .system t :: rest =>
    let (sys, others) := splitSystem rest
    (t :: sys, others)
  | m :: rest =>
    let (sys, others) := splitSystem rest
    (sys, m :: others)

def systemBlocks (preamble : Option String) (rows : List String) : List String :=
  identity :: (preamble.toList ++ rows)

theorem systemBlocks_head (preamble : Option String) (rows : List String) :
    (systemBlocks preamble rows).head? = some identity := rfl

theorem systemBlocks_tail_verbatim (preamble : Option String) (rows : List String) :
    (systemBlocks preamble rows).tail = preamble.toList ++ rows := rfl

def isSystem : Msg → Bool
  | .system _ => true
  | .other _ => false

/-- The remaining list contains no `System` row, and the split loses nothing:
the system texts are exactly the `System` rows in order. -/
theorem splitSystem_partition (msgs : List Msg) :
    (splitSystem msgs).2.all (fun m => !isSystem m) = true ∧
    (splitSystem msgs).1 = (msgs.filter isSystem).map (fun m =>
      match m with | .system t => t | .other _ => "") ∧
    (splitSystem msgs).2 = msgs.filter (fun m => !isSystem m) := by
  induction msgs with
  | nil => simp [splitSystem]
  | cons m rest ih =>
    obtain ⟨h1, h2, h3⟩ := ih
    cases m with
    | system t => simp [splitSystem, isSystem, List.filter, h1, h2, h3]
    | other tag => simp [splitSystem, isSystem, List.filter, h1, h2, h3]

/-- `tools` is absent from the wire for an empty surface. -/
def toolsField : List String → Option (List String)
  | [] => none
  | tools => some tools

theorem toolsField_empty : toolsField [] = none := rfl

theorem toolsField_nonempty (t : String) (rest : List String) :
    toolsField (t :: rest) = some (t :: rest) := rfl
```

If `splitSystem_partition` does not close with `simp` as written, split it into three theorems (`splitSystem_no_system_left`, `splitSystem_systems`, `splitSystem_others`) each by the same induction; the obligation is the same. Zero `sorry`.

- [ ] **Step 3: Append the stream-accumulation model to `ClaudeMap.lean`**

```lean
/-! ## Tool-block accumulation (SSE)

One `tool_use` block arrives as `content_block_start` (with a usually-empty
`input`), zero or more `input_json_delta` fragments, and `content_block_stop`.
Defect C1 seeded the start input and appended deltas (`{}{...}`). Here the
deltas are the arguments whenever any arrived. -/

inductive StreamEvent where
  | text (t : String)
  | start (id : ToolCallId) (name : String) (input : Option String)
  | delta (partial : String)
  | stop
  deriving DecidableEq, Repr

def accumulate (start : Option String) (deltas : List String) : String :=
  match deltas with
  | [] => start.getD "{}"
  | _ => String.join deltas

theorem accumulate_ignores_start_when_streamed (start : Option String)
    (deltas : List String) (h : deltas ≠ []) :
    accumulate start deltas = String.join deltas := by
  cases deltas with
  | nil => exact absurd rfl h
  | cons d rest => rfl

theorem accumulate_uses_start_when_no_deltas (start : Option String) :
    accumulate start [] = start.getD "{}" := rfl

structure Pending where
  id : ToolCallId
  name : String
  start : Option String
  /-- Deltas in reverse arrival order (consed); flushed as `deltas.reverse`. -/
  deltas : List String
  deriving Repr

structure StreamState where
  pending : Option Pending
  seen : List ToolCallId
  out : List (ToolCallId × String)
  deriving Repr

def StreamState.init : StreamState := { pending := none, seen := [], out := [] }

/-- Flush the pending block: duplicate id, then surface map, then arguments. -/
def flush (surface : Surface) (st : StreamState) : Except MapError StreamState :=
  match st.pending with
  | none => .ok st
  | some p =>
    if p.id ∈ st.seen then
      .error (.duplicateId p.id)
    else
      match mapToolUse surface p.id p.name with
      | .error e => .error e
      | .ok _ =>
        .ok { pending := none
            , seen := p.id :: st.seen
            , out := st.out ++ [(p.id, accumulate p.start p.deltas.reverse)] }

def step (surface : Surface) (st : StreamState) : StreamEvent → Except MapError StreamState
  | .text _ => .ok st
  | .start id name input =>
    match st.pending with
    | some _ => .error (.overlappingBlock id)
    | none => .ok { st with pending := some { id := id, name := name, start := input, deltas := [] } }
  | .delta partial =>
    match st.pending with
    | none => .ok st
    | some p => .ok { st with pending := some { p with deltas := partial :: p.deltas } }
  | .stop => flush surface st

/-- Left to right, first error wins; end of stream flushes an unterminated block. -/
def runStream (surface : Surface) (events : List StreamEvent) :
    Except MapError (List (ToolCallId × String)) :=
  (events.foldlM (step surface) StreamState.init >>= flush surface) |>.map (·.out)

theorem runStream_text_only (surface : Surface) :
    runStream surface [.text "hi"] = .ok [] := rfl

theorem runStream_deltas_win :
    runStream {("echo" : String)}
      [.start 1 "echo" (some "{}"), .delta "{\"text\":", .delta " \"hi\"}", .stop] =
      .ok [(1, "{\"text\": \"hi\"}")] := by
  native_decide

theorem runStream_start_input_without_deltas :
    runStream {("echo" : String)} [.start 1 "echo" (some "{\"a\":1}"), .stop] =
      .ok [(1, "{\"a\":1}")] := by
  native_decide

theorem runStream_overlap :
    runStream {("echo" : String)} [.start 1 "echo" none, .start 2 "echo" none, .stop] =
      .error (.overlappingBlock 2) := by
  native_decide

theorem runStream_duplicate :
    runStream {("echo" : String)}
      [.start 1 "echo" none, .stop, .start 1 "echo" none, .stop] =
      .error (.duplicateId 1) := by
  native_decide

theorem runStream_unterminated_flushes :
    runStream {("echo" : String)} [.start 1 "echo" none, .delta "{}"] = .ok [(1, "{}")] := by
  native_decide
```

`ToolCallId` is a `Nat` (see `ToolExecution`); literals `1`, `2` are fine. If `native_decide` is unavailable in this project's Mathlib pin, use `decide` or `rfl`; if `String` decidability blocks `decide`, use `by simp [runStream, step, flush, accumulate, mapToolUse]`.

- [ ] **Step 4: `lake build` the model**

```bash
cd crates/gents/proofs && lake build 2>&1 | tail -5 && grep -rn 'sorry' Proofs/PromptAssembly/ClaudeMap.lean ; echo "sorry-scan exit=$?"
```
Expected: build succeeds; no `sorry`.

- [ ] **Step 5: Witnesses in `Conformance/ContractCases/PromptAssembly.lean`**

Replace `surfaceOf`:

```lean
private def surfaceOf (names : List String) : PromptAssembly.ClaudeMap.Surface :=
  names.toFinset
```

Append before `end Conformance.ContractCases`:

```lean
/-! ## Claude Messages body (single wire)

`system` is `systemBlocks preamble (splitSystem rows).1`; `toolsPresent` is
`(toolsField tools).isSome`. Rows are tagged `system:<text>` / `other:<tag>`. -/

structure PromptAssemblyClaudeBodyCase where
  name : String
  preamble : Option String
  rows : List String
  tools : List String
  system : List String
  toolsPresent : Bool
  deriving Repr

private def msgTag : PromptAssembly.ClaudeMap.Msg → String
  | .system t => "system:" ++ t
  | .other tag => "other:" ++ tag

private def claudeBodyCase (name : String) (preamble : Option String)
    (rows : List PromptAssembly.ClaudeMap.Msg) (tools : List String) :
    PromptAssemblyClaudeBodyCase :=
  let (sys, _) := PromptAssembly.ClaudeMap.splitSystem rows
  { name := name
  , preamble := preamble
  , rows := rows.map msgTag
  , tools := tools
  , system := PromptAssembly.ClaudeMap.systemBlocks preamble sys
  , toolsPresent := (PromptAssembly.ClaudeMap.toolsField tools).isSome }

def promptAssemblyClaudeBodyCases : List PromptAssemblyClaudeBodyCase :=
  [ claudeBodyCase "identity-only" none [.other "user"] []
  , claudeBodyCase "preamble-after-identity" (some "You are helpful.") [.other "user"] ["echo"]
  , claudeBodyCase "system-rows-follow-preamble" (some "P")
      [.system "S1", .other "user", .system "S2", .other "assistant"] ["echo"]
  , claudeBodyCase "system-rows-without-preamble" none
      [.system "workspace context", .other "user"] []
  , claudeBodyCase "empty-tools-omitted" (some "P") [.other "user"] []
  , claudeBodyCase "two-tools-present" none [.other "user"] ["echo", "list_files"]
  ]

/-! ## Claude Messages SSE stream (single wire)

`outcome` / `calls` are `runStream`, not a hand oracle. Event tags:
`text:<t>`, `start:<id>:<name>:<input>` (empty input = none), `delta:<partial>`, `stop`. -/

structure PromptAssemblyClaudeStreamCase where
  name : String
  surface : List String
  events : List String
  outcome : String
  calls : List String
  deriving Repr

private def streamEventTag : PromptAssembly.ClaudeMap.StreamEvent → String
  | .text t => "text:" ++ t
  | .start id name input => s!"start:{id}:{name}:{input.getD ""}"
  | .delta partial => "delta:" ++ partial
  | .stop => "stop"

private def claudeStreamCase (name : String) (surface : List String)
    (events : List PromptAssembly.ClaudeMap.StreamEvent) : PromptAssemblyClaudeStreamCase :=
  match PromptAssembly.ClaudeMap.runStream (surfaceOf surface) events with
  | .ok calls =>
    { name := name, surface := surface, events := events.map streamEventTag
    , outcome := "ok", calls := calls.map (fun (id, args) => s!"{id}={args}") }
  | .error e =>
    { name := name, surface := surface, events := events.map streamEventTag
    , outcome := PromptAssembly.ClaudeMap.errorName e, calls := [] }

def promptAssemblyClaudeStreamCases : List PromptAssemblyClaudeStreamCase :=
  [ claudeStreamCase "text-only" [] [.text "hi"]
  , claudeStreamCase "deltas-win-over-start" ["echo"]
      [.start 1 "echo" (some "{}"), .delta "{\"text\":", .delta " \"hi\"}", .stop]
  , claudeStreamCase "start-input-without-deltas" ["echo"]
      [.start 1 "echo" (some "{\"a\":1}"), .stop]
  , claudeStreamCase "no-input-is-empty-object" ["echo"] [.start 1 "echo" none, .stop]
  , claudeStreamCase "overlapping-block" ["echo"]
      [.start 1 "echo" none, .start 2 "echo" none, .stop]
  , claudeStreamCase "duplicate-id" ["echo"]
      [.start 1 "echo" none, .stop, .start 1 "echo" none, .stop]
  , claudeStreamCase "unmapped-name" ["bash"] [.start 1 "Bash" none, .stop]
  , claudeStreamCase "empty-surface-tool-use" [] [.start 1 "echo" none, .stop]
  , claudeStreamCase "unterminated-flushes" ["echo"] [.start 1 "echo" none, .delta "{}"]
  , claudeStreamCase "two-blocks-in-order" ["echo", "list_files"]
      [.start 1 "echo" none, .delta "{}", .stop, .text "then", .start 2 "list_files" none, .delta "{\"path\":\".\"}", .stop]
  ]
```

- [ ] **Step 6: JSON renderers in `Contracts/Json/PromptAssembly.lean`** (before `end Conformance.Contracts`)

```lean
def promptAssemblyClaudeBodyCaseJson (witness : PromptAssemblyClaudeBodyCase) : String :=
  "{"
    ++ "\"name\":" ++ jsonString witness.name ++ ","
    ++ "\"preamble\":" ++ jsonOptionalString witness.preamble ++ ","
    ++ "\"rows\":" ++ jsonStringArray witness.rows ++ ","
    ++ "\"tools\":" ++ jsonStringArray witness.tools ++ ","
    ++ "\"system\":" ++ jsonStringArray witness.system ++ ","
    ++ "\"tools_present\":" ++ boolString witness.toolsPresent
    ++ "}"

def promptAssemblyClaudeBodyCasesJson : String :=
  jsonArray (promptAssemblyClaudeBodyCases.map promptAssemblyClaudeBodyCaseJson)

def promptAssemblyClaudeStreamCaseJson (witness : PromptAssemblyClaudeStreamCase) : String :=
  "{"
    ++ "\"name\":" ++ jsonString witness.name ++ ","
    ++ "\"surface\":" ++ jsonStringArray witness.surface ++ ","
    ++ "\"events\":" ++ jsonStringArray witness.events ++ ","
    ++ "\"outcome\":" ++ jsonString witness.outcome ++ ","
    ++ "\"calls\":" ++ jsonStringArray witness.calls
    ++ "}"

def promptAssemblyClaudeStreamCasesJson : String :=
  jsonArray (promptAssemblyClaudeStreamCases.map promptAssemblyClaudeStreamCaseJson)
```

`boolString` comes from `Conformance.ContractCases.Types` (already opened via `open Conformance.ContractCases`); `jsonOptionalString` from `ContractTypes`.

- [ ] **Step 7: Snapshot keys and ledger**

`Contracts/Json/Snapshot.lean`, after the `prompt_assembly_claude_map_cases` pair:

```lean
    ++ "\"prompt_assembly_claude_body_cases\":"
      ++ promptAssemblyClaudeBodyCasesJson ++ ","
    ++ "\"prompt_assembly_claude_stream_cases\":"
      ++ promptAssemblyClaudeStreamCasesJson ++ ","
```

`CoverageLedger.lean`: change the existing consumer id to `conformance::prompt_assembly::generated_claude_map_cases_drive_the_messages_parser`, and add after it:

```lean
  , tagged (consumerCoverage
      "prompt_assembly_cases"
      "PromptAssemblyClaudeBodyCases"
      "conformance::prompt_assembly::generated_claude_body_cases_drive_the_body_builder")
      "prompt-assembly" [Surface.runtimeInternal]
  , tagged (consumerCoverage
      "prompt_assembly_cases"
      "PromptAssemblyClaudeStreamCases"
      "conformance::prompt_assembly::generated_claude_stream_cases_drive_the_messages_parser")
      "prompt-assembly" [Surface.runtimeInternal]
```

```bash
cd crates/gents/proofs && lake build 2>&1 | tail -3 && lake env lean --run Proofs/Conformance/Contracts.lean | grep -o '"prompt_assembly_claude_[a-z_]*_cases"' 
```
Expected: three keys printed (`map`, `body`, `stream`).

- [ ] **Step 8: Rust snapshot structs, accessors, coverage**

`crates/gents/src/lean_vocab_test/prompt_assembly.rs`, after `LeanPromptAssemblyClaudeMapCase`:

```rust
/// A Claude Messages body witness computed by `ClaudeMap.systemBlocks` /
/// `splitSystem` / `toolsField`.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct LeanPromptAssemblyClaudeBodyCase {
    pub(crate) name: String,
    pub(crate) preamble: Option<String>,
    pub(crate) rows: Vec<String>,
    pub(crate) tools: Vec<String>,
    pub(crate) system: Vec<String>,
    pub(crate) tools_present: bool,
}

/// A Claude Messages SSE witness computed by `ClaudeMap.runStream`.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct LeanPromptAssemblyClaudeStreamCase {
    pub(crate) name: String,
    pub(crate) surface: Vec<String>,
    pub(crate) events: Vec<String>,
    pub(crate) outcome: String,
    pub(crate) calls: Vec<String>,
}
```

`support.rs` snapshot struct (after `prompt_assembly_claude_map_cases`):

```rust
    #[serde(default)]
    pub(crate) prompt_assembly_claude_body_cases: Vec<LeanPromptAssemblyClaudeBodyCase>,
    #[serde(default)]
    pub(crate) prompt_assembly_claude_stream_cases: Vec<LeanPromptAssemblyClaudeStreamCase>,
```

accessors (after `lean_prompt_assembly_claude_map_cases`):

```rust
pub(crate) fn lean_prompt_assembly_claude_body_cases() -> &'static [LeanPromptAssemblyClaudeBodyCase] {
    &lean_contract_snapshot().prompt_assembly_claude_body_cases
}

pub(crate) fn lean_prompt_assembly_claude_stream_cases() -> &'static [LeanPromptAssemblyClaudeStreamCase]
{
    &lean_contract_snapshot().prompt_assembly_claude_stream_cases
}
```

`tests/conformance/coverage.rs` after the claude_map block:

```rust
    if !snapshot.prompt_assembly_claude_body_cases.is_empty() {
        emitted.insert((
            "prompt_assembly_cases".to_string(),
            "PromptAssemblyClaudeBodyCases".to_string(),
        ));
    }
    if !snapshot.prompt_assembly_claude_stream_cases.is_empty() {
        emitted.insert((
            "prompt_assembly_cases".to_string(),
            "PromptAssemblyClaudeStreamCases".to_string(),
        ));
    }
```

`tests/support/conformance_consumers.rs`: rename the existing entry's `id` and `function` to `generated_claude_map_cases_drive_the_messages_parser`, and add two entries with the same `package`/`source_path`/`module_path` for `generated_claude_body_cases_drive_the_body_builder` and `generated_claude_stream_cases_drive_the_messages_parser`.

- [ ] **Step 9: Drivers in `tests/conformance/prompt_assembly.rs`**

Replace the `claude_completer` import and extend the lean_vocab import:

```rust
use gents::claude_messages::{CLAUDE_CODE_IDENTITY, build_messages_body, parse_messages_sse};
use rig::streaming::RawStreamingChoice;

use crate::lean_vocab_test::{
    LeanPromptAssemblyItem, LeanPromptAssemblyRow, lean_prompt_assembly_claude_body_cases,
    lean_prompt_assembly_claude_map_cases, lean_prompt_assembly_claude_stream_cases,
    lean_prompt_assembly_sanitize_cases,
};
```

Replace the map driver and `claude_map_assistant_line` with:

```rust
/// Fence: the Messages SSE parser reproduces `ClaudeMap.mapTurn` on every
/// generated witness (mapped ids, empty-surface / unmapped / duplicate fail closed).
#[test]
fn generated_claude_map_cases_drive_the_messages_parser() {
    let cases = lean_prompt_assembly_claude_map_cases();
    assert!(!cases.is_empty(), "Lean emitted no PromptAssembly Claude map cases");
    for case in cases {
        let surface: HashSet<String> = case.surface.iter().cloned().collect();
        let sse = claude_map_blocks_as_sse(&case.blocks);
        let parsed = parse_messages_sse(&sse, &surface);
        match case.outcome.as_str() {
            "ok" => {
                let events = parsed.unwrap_or_else(|err| panic!("case {} should map: {err}", case.name));
                let got_ids: Vec<u64> = events
                    .iter()
                    .filter_map(|event| match event {
                        RawStreamingChoice::ToolCall(call) => Some(
                            call.id.parse::<u64>().unwrap_or_else(|_| {
                                panic!("case {} mapped id {} is not a Nat", case.name, call.id)
                            }),
                        ),
                        _ => None,
                    })
                    .collect();
                assert_eq!(got_ids, case.ids, "mapped ids ({})", case.name);
            }
            "emptySurface" => {
                let err = parsed.expect_err("empty surface must fail closed").to_string();
                assert!(err.contains("fail-closed: tool_use observed"), "case {}: {err}", case.name);
            }
            outcome if outcome.starts_with("unmappedName:") => {
                let name = outcome.strip_prefix("unmappedName:").expect("prefix");
                let err = parsed.expect_err("unmapped name must fail closed").to_string();
                assert!(
                    err.contains("fail-closed: tool_use observed") && err.contains(name),
                    "case {}: {err}",
                    case.name
                );
            }
            outcome if outcome.starts_with("duplicateId:") => {
                let id = outcome.strip_prefix("duplicateId:").expect("prefix");
                let err = parsed.expect_err("duplicate id must fail closed").to_string();
                assert!(
                    err.contains(&format!("fail-closed: duplicate tool_use id {id}")),
                    "case {}: {err}",
                    case.name
                );
            }
            other => panic!("case {} unknown outcome {other}", case.name),
        }
    }
}

fn sse_event(payload: serde_json::Value) -> String {
    let kind = payload["type"].as_str().expect("type").to_string();
    format!("event: {kind}\ndata: {payload}\n\n")
}

/// Render ClaudeMap block tags (`text`, `toolUse:id:name`, `toolResult:id`) as
/// an assistant-turn SSE body. `toolResult` never appears in an assistant turn
/// and is skipped.
fn claude_map_blocks_as_sse(blocks: &[String]) -> String {
    let mut sse = String::new();
    for (index, block) in blocks.iter().enumerate() {
        if block == "text" {
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_start", "index": index,
                "content_block": {"type": "text", "text": ""}
            })));
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_delta", "index": index,
                "delta": {"type": "text_delta", "text": "hi"}
            })));
            sse.push_str(&sse_event(serde_json::json!({"type": "content_block_stop", "index": index})));
        } else if let Some(rest) = block.strip_prefix("toolUse:") {
            let (id, name) = rest
                .split_once(':')
                .unwrap_or_else(|| panic!("toolUse tag must be toolUse:id:name, got {block}"));
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_start", "index": index,
                "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}
            })));
            sse.push_str(&sse_event(serde_json::json!({"type": "content_block_stop", "index": index})));
        } else if block.starts_with("toolResult:") {
            continue;
        } else {
            panic!("unknown Claude map block tag {block}");
        }
    }
    sse.push_str(&sse_event(serde_json::json!({"type": "message_stop"})));
    sse
}

/// Fence: the Messages SSE parser reproduces `ClaudeMap.runStream` — deltas
/// win over the start input, overlap / duplicate / unmapped fail closed, an
/// unterminated block flushes at end of stream.
#[test]
fn generated_claude_stream_cases_drive_the_messages_parser() {
    let cases = lean_prompt_assembly_claude_stream_cases();
    assert!(!cases.is_empty(), "Lean emitted no PromptAssembly Claude stream cases");
    for case in cases {
        let surface: HashSet<String> = case.surface.iter().cloned().collect();
        let sse = claude_stream_events_as_sse(&case.events);
        let parsed = parse_messages_sse(&sse, &surface);
        match case.outcome.as_str() {
            "ok" => {
                let events = parsed.unwrap_or_else(|err| panic!("case {} should parse: {err}", case.name));
                let got: Vec<String> = events
                    .iter()
                    .filter_map(|event| match event {
                        RawStreamingChoice::ToolCall(call) => Some(format!("{}={}", call.id, call.arguments)),
                        _ => None,
                    })
                    .collect();
                let want: Vec<String> = case
                    .calls
                    .iter()
                    .map(|entry| {
                        let (id, args) = entry.split_once('=').expect("id=args");
                        let value: serde_json::Value = serde_json::from_str(args)
                            .unwrap_or_else(|err| panic!("case {} witness args {args}: {err}", case.name));
                        format!("{id}={value}")
                    })
                    .collect();
                assert_eq!(got, want, "calls ({})", case.name);
            }
            "emptySurface" => {
                let err = parsed.expect_err("empty surface must fail closed").to_string();
                assert!(err.contains("fail-closed: tool_use observed"), "case {}: {err}", case.name);
            }
            outcome if outcome.starts_with("unmappedName:") => {
                let name = outcome.strip_prefix("unmappedName:").expect("prefix");
                let err = parsed.expect_err("unmapped must fail closed").to_string();
                assert!(err.contains("fail-closed: tool_use observed") && err.contains(name), "case {}: {err}", case.name);
            }
            outcome if outcome.starts_with("duplicateId:") => {
                let id = outcome.strip_prefix("duplicateId:").expect("prefix");
                let err = parsed.expect_err("duplicate must fail closed").to_string();
                assert!(err.contains(&format!("fail-closed: duplicate tool_use id {id}")), "case {}: {err}", case.name);
            }
            outcome if outcome.starts_with("overlappingBlock:") => {
                let id = outcome.strip_prefix("overlappingBlock:").expect("prefix");
                let err = parsed.expect_err("overlap must fail closed").to_string();
                assert!(err.contains(&format!("fail-closed: overlapping tool_use block {id}")), "case {}: {err}", case.name);
            }
            other => panic!("case {} unknown outcome {other}", case.name),
        }
    }
}

/// Render `StreamEvent` tags as SSE. `start:<id>:<name>:<input>` with empty
/// input emits no `input` key (Lean `none`); a non-empty input is embedded as
/// the parsed JSON object.
fn claude_stream_events_as_sse(events: &[String]) -> String {
    let mut sse = String::new();
    for tag in events {
        if let Some(text) = tag.strip_prefix("text:") {
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_delta", "index": 0,
                "delta": {"type": "text_delta", "text": text}
            })));
        } else if let Some(rest) = tag.strip_prefix("start:") {
            let mut parts = rest.splitn(3, ':');
            let id = parts.next().expect("id");
            let name = parts.next().expect("name");
            let input = parts.next().unwrap_or("");
            let mut block = serde_json::json!({"type": "tool_use", "id": id, "name": name});
            if !input.is_empty() {
                block["input"] = serde_json::from_str(input)
                    .unwrap_or_else(|err| panic!("witness start input {input}: {err}"));
            }
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_start", "index": 0, "content_block": block
            })));
        } else if let Some(partial) = tag.strip_prefix("delta:") {
            sse.push_str(&sse_event(serde_json::json!({
                "type": "content_block_delta", "index": 0,
                "delta": {"type": "input_json_delta", "partial_json": partial}
            })));
        } else if tag == "stop" {
            sse.push_str(&sse_event(serde_json::json!({"type": "content_block_stop", "index": 0})));
        } else {
            panic!("unknown Claude stream event tag {tag}");
        }
    }
    sse
}

/// Fence: `build_messages_body` reproduces `ClaudeMap.systemBlocks` /
/// `splitSystem` / `toolsField` — identity first, preamble, then `System`
/// rows in order; `tools` absent for an empty surface.
#[test]
fn generated_claude_body_cases_drive_the_body_builder() {
    use rig::completion::message::{Message, Text, UserContent};
    use rig::one_or_many::OneOrMany;
    let cases = lean_prompt_assembly_claude_body_cases();
    assert!(!cases.is_empty(), "Lean emitted no PromptAssembly Claude body cases");
    for case in cases {
        let history: Vec<Message> = case
            .rows
            .iter()
            .map(|row| {
                if let Some(text) = row.strip_prefix("system:") {
                    Message::System { content: text.to_string() }
                } else if row == "other:assistant" {
                    Message::assistant("ok")
                } else {
                    Message::User {
                        content: OneOrMany::one(UserContent::Text(Text { text: "hi".into() })),
                    }
                }
            })
            .collect();
        let request = rig::completion::CompletionRequest {
            model: None,
            preamble: case.preamble.clone(),
            chat_history: OneOrMany::many(history).expect("at least one row"),
            documents: Vec::new(),
            tools: case
                .tools
                .iter()
                .map(|name| rig::completion::ToolDefinition {
                    name: name.clone(),
                    description: name.clone(),
                    parameters: serde_json::json!({"type": "object"}),
                })
                .collect(),
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        };
        let body = build_messages_body("claude-sonnet-5", &request);
        let system: Vec<String> = body["system"]
            .as_array()
            .unwrap_or_else(|| panic!("case {} system array: {body}", case.name))
            .iter()
            .map(|block| block["text"].as_str().expect("text").to_string())
            .collect();
        assert_eq!(system, case.system, "system blocks ({})", case.name);
        assert_eq!(system[0], CLAUDE_CODE_IDENTITY, "case {}", case.name);
        assert_eq!(body.get("tools").is_some(), case.tools_present, "tools key ({})", case.name);
        let leaked = body["messages"].as_array().expect("messages").iter().any(|message| {
            message["content"].as_array().is_some_and(|blocks| {
                blocks.iter().any(|block| block["text"].as_str().is_some_and(|t| t.starts_with("system: ")))
            })
        });
        assert!(!leaked, "System row leaked into messages ({}): {body}", case.name);
    }
}
```

Add `use std::collections::HashSet;` to the file's imports if it is not there. `Message::System { content }` is the rig variant already matched in `anthropic_messages`.

- [ ] **Step 10: Vocab test for the identity constant**

In `crates/gents/src/claude_messages.rs` `mod tests`, add:

```rust
    /// Lean `ClaudeMap.identity` and Rust `CLAUDE_CODE_IDENTITY` are the same
    /// bytes; the body-cases fence checks `system[0]` against the witness, so
    /// this pins the constant even for a witness set with no rows.
    #[test]
    fn identity_matches_lean_body_witness_head() {
        let cases = crate::lean_vocab_test::lean_prompt_assembly_claude_body_cases();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(case.system[0], CLAUDE_CODE_IDENTITY, "{}", case.name);
        }
    }
```

If `lean_vocab_test` is only compiled under `#[cfg(test)]` in `lib.rs` (check `crates/gents/src/lib.rs` for `mod lean_vocab_test`), this test compiles as written.

- [ ] **Step 11: Run and record the expected red**

```bash
cargo test -p gents --test conformance -- prompt_assembly::generated_claude 2>&1 | tail -25
cargo test -p gents --lib claude_messages 2>&1 | tail -8
```
Expected:
- `generated_claude_map_cases_drive_the_messages_parser` PASS
- `generated_claude_stream_cases_drive_the_messages_parser` PASS (Task 1 already implements accumulate/dup/overlap)
- `generated_claude_body_cases_drive_the_body_builder` **FAIL** on `system-rows-follow-preamble` and `system-rows-without-preamble` (System rows currently land in `messages` as `system: …` user blocks). This is the fence for Task 6.
- `identity_matches_lean_body_witness_head` PASS.

If the conformance test binary is named differently, find it with `ls crates/gents/tests/*.rs` and use `--test <name>`.

- [ ] **Step 12: Gates and commit**

```bash
cargo test -p gents 2>&1 | grep -E 'test result|FAILED|failed' | head
cargo check --workspace --all-targets 2>&1 | tail -3
git add crates/gents/proofs crates/gents/src/lean_vocab_test crates/gents/tests crates/gents/src/claude_messages.rs
git commit -m "spec(claude): model system assembly and SSE tool-block accumulation; fence the Messages parser

ClaudeMap gains splitSystem/systemBlocks/toolsField and a StreamEvent
accumulation model (deltas win over start input; overlap, duplicate and
unmapped fail closed). Two witness sets drive build_messages_body and
parse_messages_sse; the existing map witnesses are re-pointed from the
JSONL parser to the Messages parser. The body fence is red on System-row
placement until the single-wire cut lands."
```
Expected: exactly one failing test in the package (`generated_claude_body_cases_drive_the_body_builder`); say so in the commit's PR notes. Do not add `docs/superpowers/`.


# CryoDB architecture

CryoDB has three frontends over shared database and configuration code:

```text
GUI (Iced) ─┐
TUI ────────┼─> model + db + storage + ai + utils
CLI ────────┘
```

The GUI uses The Elm Architecture: state produces a view, messages describe events, and update functions change state or start asynchronous work. It is organized hierarchically by feature so that the whole application does not become one global state, message, update, and view module.

## Directory responsibilities

```text
src/
├── app/
│   ├── core.rs             Application composition and startup
│   ├── message.rs          Top-level feature routing
│   ├── shell.rs            Window, panes, global overlays and global input
│   ├── session.rs          Connection and database session snapshots
│   ├── coordinator/        Workflows involving multiple feature states
│   └── features/
│       ├── connections/
│       ├── workspace/
│       │   ├── explorer/
│       │   ├── query/
│       │   ├── results/
│       │   ├── diagram/
│       │   └── tabs.rs
│       ├── ai/
│       ├── transfer/
│       ├── settings/
│       ├── updater/
│       └── onboarding/
├── model/                  Frontend-neutral data and domain rules
├── db/                     Database connections and operations
├── storage/                Persistence and secret storage
├── ai/                     Provider clients shared by frontends
├── ui/                     Shared presentation primitives
├── tui/
├── cli.rs
└── utils/
```

### Composition root

`app/core.rs` owns the composed application state:

```rust
pub(crate) struct App {
    shell: shell::State,
    connections: connections::State,
    ai: ai::State,
    workspace: workspace::State,
    settings: settings::State,
    transfer: transfer::State,
    updater: updater::State,
    onboarding: onboarding::State,
}
```

It initializes those states, builds the top-level view, and combines subscriptions. It must remain small. New feature-specific fields do not belong in `App`; add them to the state that owns them.

`app/message.rs` is only a routing enum:

```rust
pub(crate) enum Message {
    Shell(shell::Message),
    Connections(connections::Message),
    Workspace(workspace::Message),
    Ai(ai::Message),
    Transfer(transfer::Message),
    Settings(settings::Message),
    Updater(updater::Message),
    Onboarding(onboarding::Message),
}
```

Do not add leaf interactions such as `QueryChanged`, `ExportStarted`, or `ThemeSelected` to this enum. They belong to the corresponding feature message.

### Features

A feature owns its local state and event vocabulary. It also owns its update, view, and effects when those operations need only that feature.

Start with the smallest useful shape:

```text
feature/
└── mod.rs      State + Message + update
```

Split files only when the feature becomes difficult to navigate:

```text
feature/
├── mod.rs      State + Message + update routing
├── view.rs     Feature UI
├── effects.rs  File, network or other asynchronous work
└── tests.rs    Focused state-transition tests
```

Do not create every possible file for a small feature. Do not split by technical layer when one cohesive module is clearer.

`workspace` is a parent feature. It owns explorer, query, results, diagram, and tab state because those areas share one active database session. These children remain separate owners even though many database workflows involve more than one of them.

### Coordinators

`app/coordinator` contains workflows that must inspect or update multiple composed states. Examples include running a query and placing its result in the results feature, selecting an explorer table and opening a tab, or switching a connection and restoring its workspace.

Add code to a coordinator only when a single feature cannot perform the operation without importing a sibling or receiving an overly broad context. A coordinator may operate on `App`; a feature must not.

Coordinator files are grouped by the workflow they coordinate. They are not a second home for arbitrary feature logic. A rule that uses only `results::State`, for example, belongs in the results feature even if an existing coordinator calls it.

### Shell and sessions

`app/shell.rs` owns application-wide presentation and input state: pane layout, window dimensions, global overlays, toasts, tooltips, global pointer state, and keyboard routing.

`app/session.rs` owns the snapshots used when switching connections or databases. Session data must stay isolated by connection and database. Async results must be checked against their originating session before they are applied.

Do not place feature forms, database metadata, query results, or AI conversations in shell state.

### Shared frontend-neutral code

`model`, `db`, `storage`, `ai`, and reusable utilities must remain independent of the GUI. They may not depend on `App`, GUI feature messages, Iced widgets, or `Element`.

- `model` contains persisted types and domain rules.
- `db` performs database operations and returns data or typed errors.
- `storage` reads and writes configuration, connection, folder, and diagram data.
- `ai` communicates with providers and parses provider responses.
- `utils` contains logic with no clear domain owner that is reused in more than one place.

GUI-specific state such as modal visibility, selection, animation, drag state, and widget content stays under `app`.

`ui` contains shared presentation primitives. A widget belongs in `ui/widgets` only when it is presentation-only and reused, or is a genuinely generic primitive. It must not accept `&App`, perform I/O, or emit the root `Message` directly.

## Dependency rules

Dependencies flow inward toward shared code:

```text
main
└── app root
    ├── coordinator
    │   └── features
    ├── features
    │   └── model + db + storage + ai + utils
    └── ui

tui + cli
└── model + db + storage + ai + utils
```

Use these rules for every change:

1. A feature may depend on frontend-neutral modules and shared UI primitives.
2. A feature may not import a sibling feature.
3. A child requests a sibling reaction through a typed `Output` handled by its parent or a coordinator.
4. Async work returns to the feature that requested it.
5. Shared layers never return GUI messages or views.
6. Persisted formats do not change during structural refactors.
7. Use explicit imports in new or materially changed application modules.

## Message and output flow

A local interaction stays inside its feature:

```text
feature view
  -> feature::Message
  -> feature update
  -> feature state change or Task<feature::Message>
```

Views and tasks are mapped at the parent boundary:

```rust
feature::view(&state, context).map(Message::Feature)

task.map(Message::Feature)
```

A cross-feature interaction goes upward before it goes downward:

```text
AI message
  -> AI update
  -> ai::Output::RunSql(sql)
  -> root coordinator
  -> query action
  -> query task
  -> query result
  -> results state
```

Never call one sibling feature from another. Do not use a global event bus, shared mutable state, or `Arc<Mutex<_>>` to bypass ownership.

Use `Task<feature::Message>` for self-contained work. Return an `Output` when the parent must react immediately. Use a collection of outputs only when one message can legitimately produce several parent actions.

## Effects and asynchronous work

The feature that owns a request also owns its GUI effect adapter and cancellation state. The underlying operation remains in the shared layer when another frontend can reuse it.

```text
feature message
  -> feature effect creates an Iced Task
  -> db/storage/ai function performs the operation
  -> task maps the result to a feature message
  -> feature update applies the result
```

Clone only the data an async task must own. Include connection, database, tab, request, or generation identifiers when a late result could otherwise update the wrong session.

File dialogs belong to the feature that owns the selected path. Transfer progress belongs to transfer messages. AI streaming belongs to AI messages. Passive event streams use subscriptions composed at the root.

## Where new code belongs

| Change | Location |
| --- | --- |
| Local state or interaction | Owning feature `State` and `Message` |
| Local state transition | Owning feature update |
| Feature-specific UI | Owning feature view |
| Reusable presentation-only widget | `ui/widgets` |
| Workflow involving sibling states | `app/coordinator` |
| Window, pane, global overlay or global input | `app/shell.rs` |
| Connection/database restoration data | `app/session.rs` |
| Database operation | `db` |
| Persisted or domain type | `model` |
| Persistence or secrets | `storage` |
| Provider request or response parsing | `ai` |
| Logic shared by multiple unrelated modules | `utils` |

When the answer is unclear, start with the narrowest owner. Move code outward only after a second real consumer exists.

## Working on the codebase

Humans and AI agents follow the same process. The tools differ; the architectural decisions do not.

### Before editing

1. Read the files owned by the target feature.
2. Check the working tree and preserve unrelated changes.
3. Identify the state owner, message owner, effect boundary, and any required parent output.
4. Confirm whether the change is user-visible. User-visible changes require an entry under `CHANGELOG.md` → `[Unreleased]`; internal refactors do not.

Useful inspection commands:

```bash
eza -Ta --git-ignore src/app
rg "SymbolName" src
git diff --check
```

### Implementing a local feature change

1. Add or adjust the smallest feature state needed.
2. Add a local feature message describing the event or result.
3. Handle it in the feature update.
4. Start external work through a feature effect and map its result back to the same feature.
5. Render the state in the feature view and map its message at the parent boundary.
6. Add a focused behavioral test when the transition or failure mode needs protection.

Most local changes should not modify `App` or the root `Message`.

### Implementing a cross-feature change

1. Keep the initiating event local.
2. Emit a typed output that describes the requested action.
3. Handle that output in the parent or the narrowest coordinator.
4. Invoke the receiving feature through its message or a focused coordinator operation.
5. Preserve session identity across asynchronous boundaries.

Do not give the initiating feature access to all of `App` for convenience.

### Reviewing a change

Verify ownership before style:

- Is each new field in the narrowest state that owns it?
- Are leaf messages local to their feature?
- Does any feature import a sibling?
- Does shared code depend on Iced or `app`?
- Could a late task result affect another connection, database, or tab?
- Did a one-off abstraction add more surface area than it removed?
- Were persisted formats and existing behavior preserved?
- Is every touched file still easy to navigate?

Large files are a signal to split by cohesive responsibility. An application file must not exceed 3,000 lines; split it earlier when navigation becomes difficult. Keep closely related state and transitions together.

## Validation

Run the standard checks after a structural or behavioral change:

```bash
cargo fmt -- --check
cargo check --locked
cargo test --locked
git diff --check
```

Use focused tests while iterating, then run the full suite before declaring the work complete.

Compilation and unit tests prove source-level integrity. They do not prove visual layout, live database behavior, file dialogs, external AI providers, update installation, or cross-platform packaging. Validate the real runtime flow when the change affects one of those areas.

## Practices to avoid

- Adding feature fields directly to `App`.
- Adding leaf variants to the root `Message`.
- Moving code without moving its ownership.
- Letting a feature view or update accept `&App`.
- Importing sibling feature state or messages.
- Returning root messages from shared layers.
- Hiding persistence failures.
- Creating generic component, reducer, service-locator, or command-bus frameworks.
- Adding traits without a second implementation or a necessary test seam.
- Extracting one-use widgets or helpers prematurely.
- Mixing structural refactors with unrelated behavior or visual redesigns.

The preferred change is localized, explicit, and easy to remove or review. Add abstractions only after the code demonstrates that they are needed.

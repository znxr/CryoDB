# Changelog

All notable changes to CryoDB are documented here.

This file is **compiled into the binary** and rendered by the in-app "What's new"
modal, so every published release must have a section here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Write entries for users, not for the commit log: say what changed in the app,
not which module was refactored.

## [Unreleased]

### Changed

- CryoSQL is now CryoDB. Your connections, settings and saved passwords carry over.
- Updates and installers now come from the CryoDB releases page on GitHub.
- Both themes now use a full bottom bar with sidebar and AI chat toggles.
- Schema Diagram uses a clearer icon throughout the app.
- Schema autocomplete now opens with Ctrl+Space so Tab indents in the query editor again. Tab still accepts an AI inline suggestion when one is showing.
- The table DDL view and the SQL blocks in AI chat now use the same code editor as the query editor.
- The query editor can be resized much shorter than before.
- The query editor draws a border in the Original theme, and the line-number gutter is transparent in both themes.
- Modern theme tabs are shorter, which also shortens the sidebar header.
- The query editor opens at a compact fixed height until the split is dragged.
- The schema autocomplete list is drawn over the window, so it keeps its full size no matter how short the editor is. Arrow keys move through it, Enter or Tab accepts, Escape closes.
- The CryoDB command palette moved to Ctrl+Shift+O, leaving Ctrl+Shift+P to the query editor's own palette. A stored Ctrl+Shift+P binding is migrated to the new default.
- Escape releases the query editor, so app shortcuts the editor binds itself — such as the sidebar search — work again without leaving the keyboard.
- Modern theme inactive tabs keep a muted underline, so the tab strip reads as one rail. The underline is a single pixel, matching the sidebar header's, and the tabs sit flush against each other.
- The sidebar and AI chat show a grip on their resize edge in the Original theme, and pane splits are easier to grab.
- Dragging a tab now carries the tab under the pointer and highlights the slot it will drop into.

### Fixed

- PostgreSQL query results now display `smallint`, `uuid`, `json`, `jsonb`, `interval`, `inet`, `money`, `oid`, bit, enum, `vector` and numeric array values instead of `<unprintable>`, and `numeric` values keep their declared decimal places.
- The Default accent color now restores the color chosen during onboarding.
- The query editor keeps line numbers, scrolling and keyboard commands aligned during large edits and resizing.
- Query autocomplete and formatting are available from the command palette.
- Run executes complete SQL scripts, while Run Selection executes the selection or current statement.
- Scripts with multiple row-returning statements now show horizontally scrollable result tabs for each result set.
- AI completions appear inline at the query cursor before acceptance.
- Result pagination and edit controls only appear when they can be used.
- Modern result tables fill the available width.
- Result column names remain fully visible without changing saved custom widths.
- Bottom-bar controls are vertically centered in the Modern theme.
- Original-theme diagrams no longer show a duplicate status bar.
- Database icons now follow the current theme instead of using driver-specific colors.
- Result grid display options now use fixed defaults.
- Foreign-key links are hidden for NULL values.
- Sidebars close automatically before becoming too narrow to use.
- Resizing the query editor no longer stays stuck to the pointer after the mouse button is released outside the window.
- Modern theme diagrams no longer reserve room for the Columns panel, which was squeezing the canvas.
- Dragging a tab is easier to start, previews the new position in place, and no longer latches when released away from the tab strip.

## [0.3.3] - 2026-09-01

### Added

- The query editor now understands individual statements: pressing Run executes only the statement the cursor is in, so a file with multiple queries no longer requires selecting each one by hand.
- Multi-statement scripts (e.g. CREATE TABLE followed by INSERT) now run without errors — each statement is sent to the database in sequence on a single connection.
- Text selections still work as before: selecting any range and pressing Run executes exactly that selection.

### Fixed

- On Windows, updating from the "Update available" banner installs the new version instead of restarting into the old one, and now reports an error when the update cannot be installed.
- Multi-statement SQL scripts now keep nested routine blocks, transaction modes, and trailing comments intact.

## [0.3.2] - 2026-08-31

### Added

- New Schema Diagram tab draws the whole database as an ER diagram: every table with its columns, primary keys, foreign keys, and arrows for each relation. Open it with the diagram button next to Chat with AI, or from the command palette. Several diagrams can be open at once, each in its own tab.
- The diagram canvas pans by dragging, zooms with the wheel or the toolbar, and lets tables be dragged into place, collapsed to just their name, or snapped to a grid. Clicking a table isolates its relations, clicking a relation jumps to the table on the other end, and hovering one highlights it.
- The diagram minimap now shows a camera rectangle for the visible area that can be dragged to move around large schemas.
- Diagram toolbar adds auto layout, expand and collapse all tables, search for a table or column, fullscreen, and export to SVG or PNG.
- Diagrams can be edited: create and rename tables, add and drop columns, draw a foreign key by dragging from a selected table's column onto another table, and remove one from the context menu. Pending edits are listed as a schema diff with the generated SQL to preview before applying.
- Areas group tables on the diagram: moving an area carries everything inside it, a table joins or leaves an area by being dragged in or out, resizing only changes the boundary, and an area can be locked, have its contents locked, collapsed to hide its tables and relations, or deleted without touching the tables.
- Notes are proper sticky notes now: wrapped multi-line text, a header to drag them by, and editing from the context menu. A note written from a table's context menu stays attached to it, with a leader line back to the table.
- Saved diagrams are listed in a picker at the start of the diagram toolbar, per connection and database, with buttons to save the current diagram under a name and to delete a saved one.
- Diagram relations are drawn in crow's foot notation, so the many side and the one side of every foreign key are readable at a glance, and a table that references itself now draws a proper loop.
- Diagram columns show whether they accept NULL, and the relations button switches between showing every relation, only the selected table's, or letting the diagram decide.
- A DDL button beside the Schema Diagram button shows the CREATE statement for the table you are on, syntax highlighted and ready to copy. MySQL and SQLite return their stored definition; PostgreSQL is rebuilt from the catalog.
- Primary and foreign key columns are marked with key and link icons in their own colours.
- Tabs now carry an icon for their kind: query, table, or diagram.
- Foreign keys can span several columns: draw one link, then hold Shift while drawing the next between the same two tables and both column pairs become one composite key. The status bar says so while a link is being dragged.
- Chat with AI can now change the schema: `diagram-add-table`, `diagram-add-column` and `diagram-link` queue a new table, column or foreign key as a pending change, which you review in the diff and apply yourself.
- Query tabs can be renamed from their right-click menu, and a renamed tab reads as "name — Query". The name lasts for the session only; save the query to favorites to keep it.
- A column's type, nullability and default can now be changed from a table's context menu on MySQL, MariaDB and PostgreSQL.
- Chat with AI can now work the diagram itself: it opens the schema diagram, focuses a table, rearranges it, draws and locks named areas, and pins notes, one step at a time. A pointer travels across the canvas, picks each table up and carries it into place, the camera follows it, and the chat shows what it just did.
- Areas now each carry their own colour instead of all wearing the accent, and the minimap draws them alongside the tables.
- The assistant's pointer is drawn above everything else in its own colours, so it stays readable over any table, any area colour and any theme, and the camera leans in while it works.
- Asking the assistant for a better layout now groups the diagram by area and by what is related to what, packing each cluster on its own instead of stretching one chain across the canvas.
- Chat with AI can now follow or leave the diagram agent's cursor and camera movement while it works.
- Settings now includes Saved diagrams, where every saved diagram can be reviewed and deleted at once without opening a diagram.

### Changed

- Tab titles read their kind only when it adds something: a diagram tab shows "— Diagram" once it is saved under a name, table tabs show "— Table", and query tabs stay as they are until they carry a name of their own.
- Relation names sit on their line instead of below it, and drop out entirely where several relations run close enough together for their names to pile up.
- Saved diagrams in Settings fill the panel down to the bottom of the modal, with a separator under each row, and no longer repeat the diagram count above the table.
- Keyboard shortcuts in Settings are laid out as a table with Action and Shortcut columns and a separator under each row, filling the panel down to the bottom of the modal.
- Each shortcut now reads as separate key caps joined by a plus sign instead of one line of text.
- Saved diagrams load a page at a time as the list is scrolled, so a large collection no longer builds every row up front.
- The diagram search button now expands into the search field instead of opening a second one beside it, and pressing it again collapses the field back into the button.
- Large schemas stay responsive: relations collapse to the selected table once a diagram passes 160 of them, off-screen tables and relations are skipped, and zooming far out draws tables as plain blocks instead of shaping text nobody can read.
- Diagram zoom, scale, table and relation counts moved to the status bar, leaving the toolbar to the actions and giving the canvas its full height.
- The Schema Diagram button now lives only next to Chat with AI; the duplicate in the sidebar tools menu is gone.
- Auto layout now places each table one column to the right of everything it references and orders rows to reduce crossings, instead of reproducing the grid it started from.
- Canvas drawing is antialiased, so relation curves and table outlines are smooth.
- Saved diagrams are presented in a scrollable table by connection and database, with bulk deletion in the section header.
- The diagram toolbar keeps one expand-or-collapse control, removes duplicate actions, and uses an explicit relation visibility selector.

### Removed

- The command palette prefix field is gone from Settings; set it from the command palette instead.
- The banner that appeared under the shortcut list while binding a key is gone; the key row itself already says how to cancel.

### Fixed

- Applying diagram changes now stays with the diagram and connection where it began.
- SQLite diagram edits now affect the selected attached database instead of the main database.
- Altering MySQL and MariaDB columns now keeps their existing column attributes.
- PostgreSQL diagrams now draw every column pair in composite foreign keys correctly.
- The table DDL preview can now be scrolled and its text selected for copying.
- Diagram drafts are no longer saved automatically. Save them explicitly under a name; saved diagrams stay scoped to their connection and database.
- Diagram areas can now be renamed and cycled through seven colours.
- Diagram notes now have a multiline editor and can be resized from the canvas.
- Diagram search now provides previous and next navigation with a current-match counter.
- Diagrams now support fit-to-selection and canvas shortcuts for fit, zoom, layout, relations, snap and search.
- Scrolling over the minimap now zooms around the pointed location.
- Relation lines now show their constraint names and distinguish one-to-one from many-to-one links.
- Diagram columns now show single-column unique constraints and indexes.
- Renaming a table from the Schema Diagram now applies the rename to the database, and schema changes roll back together on SQLite and PostgreSQL when one fails.
- Pending diagram edits can be undone, redone or removed individually, and deleting a saved diagram now asks for confirmation.
- SQLite connections older than 3.35 now explain that dropping columns is unsupported before any pending schema change runs.
- SQLite diagrams can now alter a column's type, nullability and default, rebuilding the table while keeping its data, checks, collations, indexes and triggers. Altering a column is no longer limited to MySQL, MariaDB and PostgreSQL.
- SQLite diagrams can add foreign keys by safely rebuilding the table while preserving its data, stored definition, indexes and triggers.
- SQLite diagrams can remove table-level, inline and composite foreign keys without losing the table's data or other schema objects.
- Diagram tabs can be pinned like query and table tabs, and show the pin badge and hide their close button when they are.
- Hiding the sidebar no longer hides the AI chat alongside it when the query editor is not on screen.
- Dropping a column is now picked from a list instead of one context-menu entry per column, which was unusable on wide tables.
- Diagram dialogs no longer close when you click inside them.
- The diagram status bar no longer stretches to fill the view in the Original theme, and its toolbar lines up with the tab strip above it.
- The last workspace tab can now be closed; a fresh query tab opens in its place instead of the close button doing nothing.
- A schema change that fails on MySQL or MariaDB no longer leaves the already applied changes pending, so applying again does not replay them; the message says how many went through.
- The sidebar and the AI chat can be resized again while a diagram tab is open.
- Notes no longer land on top of a table, and their text no longer shows through the table underneath.
- Diagram exports now use the interface font instead of falling back to a system default in the PNG, and keep the selected table and search matches highlighted.
- Assistant labels stay opaque over the diagram, relation labels no longer overlap tables, and its camera movement is slower and smoother.
- Deleting a saved diagram now removes it permanently and closes its open tabs instead of restoring it when the tab closes.
- Disconnecting now clears the query workspace, AI chat, open panels, and pending AI work before another connection is opened.
- Connection tabs now keep their AI chat state separate, including drafts, streaming replies and activity indicators.
- Relation cables now leave a clean gap around their labels without adding a visible background.

## [0.3.1] - 2026-08-28

### Added

- The database sidebar can now be toggled with `Ctrl + B` (`⌘ + B` on macOS), and the AI chat sidebar with `Ctrl + Alt + B` (`⌘ + ⌥ + B` on macOS) when AI is enabled. Both shortcuts can be changed in Settings › Keyboard.

### Changed

- The command palette now exposes the current appearance mode, interface style, manual, dark and light color palettes, and accent color, and keeps up to four recently used actions in their own section.

### Fixed

- Command and table-search palette selections now respond reliably to Enter.
- The final open workspace tab can no longer be closed.
- Reopening the sidebar now restores its saved width instead of widening it.

## [0.3.0] - 2026-08-28

### Added

- The chat now records what its mode did with each piece of SQL — left in the reply, drafted into the editor, or run — so `Ask`, `Draft to editor` and `Run read-only` are told apart at a glance instead of all looking alike.
- Models that show their reasoning now have it rendered above the answer in grey italics as it streams. Nothing is requested — models that do not volunteer reasoning simply never show the section.
- The chat now says what it is doing while it works — reading a table's columns, sampling rows, searching the schema — instead of only "Thinking...".
- The chat no longer dead-ends with "kept asking for more context" when the assistant names a table that does not exist. It is told the name is wrong, and given the closest tables that do exist, instead of being left to guess again.
- Chat replies now appear as they are written instead of after a wait on a blank panel. Works with OpenAI, Anthropic and Ollama; a local CLI provider still answers in one piece, as it always did.
- Anthropic is now a first-class AI provider: pick it in Settings › AI › Provider, paste an API key, and point the endpoint at `https://api.anthropic.com`. Chat, inline autocomplete, query fixing, and folder generation all work against it.
- The temperature sent with AI requests is now optional and off by default, so the newest reasoning models — which reject that parameter outright — no longer fail with an error. Turn it back on in Settings › AI › Provider if your model expects it.
- Inline autocomplete can now run on a local CLI provider, off by default and behind a checkbox that spells out the cost: every suggestion starts a new CLI process, so it is slow and billed per suggestion.
- Result pages can be changed from the keyboard while browsing a table: `Ctrl + Arrow Right` / `Ctrl + Arrow Left` on Linux and Windows, `⌘ + ⌥ + Arrow Right` / `⌘ + ⌥ + Arrow Left` on macOS. Both bindings can be changed in Settings › Shortcuts, and the pagination buttons now show them in their tooltips.
- Jump straight to the first or last page of a table with `Ctrl + Alt + Home` / `Ctrl + Alt + End` (`⌘ + ⌥ + Home` / `⌘ + ⌥ + End` on macOS), or with the new buttons beside the page number. Both bindings can be changed in Settings › Shortcuts. Any row filter on the table is respected, so the last page is the last page of what you are actually looking at.

### Changed

- New installs start at a 16px interface font instead of 14px, and the rest of the interface scales up with it. An existing font size is left alone; change it under Settings › Appearance.
- Connections are now marked with their database's own logo instead of a plain coloured dot, in the sidebar, the driver picker and the connection tabs alike. The colour is unchanged and still configurable per driver in Settings.

### Fixed

- `Run read-only` no longer auto-runs statements that open with `SELECT` but still write, execute code or hang the connection, such as `setval`, `load_extension`, `dblink_exec` or `pg_sleep`. They are drafted into the editor for you to review instead.
- Settings no longer reports AI inline autocomplete as active when the chosen provider is a local CLI, which cannot drive it. It now says so instead of leaving you waiting for suggestions that never arrive.
- Retyping over ground the AI already covered now reuses its last answer instead of paying for it again, and the reused suggestion appears immediately rather than after the usual pause.
- Scrolling the query editor no longer discards the AI inline suggestion you were about to accept.
- AI inline autocomplete stays quiet while the cursor is inside a string literal or a comment, where a SQL completion has nothing useful to add.
- An AI inline suggestion can now be dismissed with `Esc`, or accepted one word at a time with `Ctrl + Arrow Right` (`⌘ + Arrow Right` on macOS) when you only want part of it.
- AI inline autocomplete no longer throws away a suggestion because you kept typing while it was being generated: it drops the part you already typed and shows the rest.
- The AI inline autocomplete preview now follows the cursor: it stays put when the editor is scrolled, lands on the right row on wrapped lines, hides when the cursor scrolls out of view, and is drawn in the editor font instead of the interface one.
- AI inline autocomplete now completes the statement from the cursor instead of proposing a whole query on top of what is already written. It keeps the spacing the completion needs, inserts exactly the text shown in the preview, and stays quiet instead of reporting an error when there is nothing worth completing.
- Shortcut keys are now shown in the selected language: arrow, page and space keys are translated, and the "binding..." prompt in Settings › Shortcuts no longer stays in English.
- The CryoSQL icon is now shown correctly across Linux, macOS, and Windows.

## [0.2.2] - 2026-08-25

### Fixed

- PostgreSQL exports now preserve table names with uppercase letters, spaces, or custom schemas.

## [0.2.1] - 2026-08-17

### Added

- Release notes are shown once after a feature update, from a dismissible
  notification. Available any time under Settings → About → What's new.

### Changed

- Sidebar, panels and table headers now use consistent surface depth across
  every theme, not only Carbon Frost.

## [0.2.0] - 2026-08-16

### Added

- First-run onboarding that picks your appearance and language before the first
  connection.
- Interface translations for Spanish, Portuguese, Russian and Japanese.
- A Modern theme variant with a redesigned sidebar, tabs and result grid.
- A switch that turns every AI feature off for the whole app.

### Fixed

- Updating the app no longer fails with a duplicate-install error.

## [0.1.3] - 2026-07-27

### Added

- AI chat sidebar that can read your schema to answer questions about it.
- Per-provider AI autocomplete with schema context in the query editor.

### Changed

- The settings modal opens and switches sections noticeably faster.

## [0.1.2] - 2026-07-13

### Added

- The running version is now visible in the app.
- Folders can be renamed.
- Each settings section can be reset on its own.

## [0.1.1] - 2026-07-06

### Added

- First public release: MySQL, MariaDB, PostgreSQL and SQLite connections, the
  query editor, the result grid and the signed auto-updater.

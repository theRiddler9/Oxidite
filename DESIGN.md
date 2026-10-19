# DESIGN.md --- Zmux-Style Terminal Workspace

## 1. Product Intent

Build a dense, keyboard-first developer workspace inspired by the
supplied reference screenshot. The interface should feel like a
professional terminal/IDE cockpit: multiple repositories and AI coding
sessions remain visible at once, code changes are reviewable in a split
diff, and Git actions are available without leaving the workspace.

This is a **visual and interaction specification**, not permission to
copy any third-party branding. Use original product naming and icons
while preserving the reference's layout, information density, and
workflow.

### Design principles

-   **Information-dense, not cluttered:** compact rows, tight spacing,
    clear alignment, minimal decoration.
-   **Dark-first:** near-black surfaces, subtle separators, restrained
    syntax and status colors.
-   **Persistent context:** workspaces, chats, diffs, and Git state
    remain visible together.
-   **Keyboard-first:** navigation and common actions must have
    discoverable shortcuts.
-   **Trustworthy change review:** additions, deletions, staged state,
    and repository state must be visually unambiguous.
-   **No decorative dashboard UI:** avoid oversized cards, gradients,
    giant headings, excessive rounding, empty hero areas, and
    unnecessary charts.

## 2. Overall Layout

Target a desktop application at **1440 × 900 px and above**. The
reference is an ultra-wide desktop composition with five primary
vertical regions.

``` text
┌────────────────────┬───────────────────┬──────────────────────────────┬────────────────────┐
│ Workspace sidebar  │ Agent / chat pane │ Main editor / diff workspace │ Git / changes pane  │
│                    │                   │                              │                    │
│ Repositories       │ Session tabs       │ File tabs / diff tabs         │ Changed files       │
│ Branches / status  │ Agent transcript   │ Side-by-side code diff        │ Stage controls      │
│ Quick actions      │ Prompt composer     │                              │ Commit / fetch      │
│ Chat sessions      │                   │                              │                    │
└────────────────────┴───────────────────┴──────────────────────────────┴────────────────────┘
```

Use a **five-region layout**: 1. **Workspace rail:** approximately 260
px wide. 2. **Agent/chat panel:** approximately 285 px wide. 3. **Main
code/diff area:** flexible, takes the majority of remaining width. 4.
**Changes/Git panel:** approximately 300 px wide. 5. **Top chrome:** a
compact shared header across the application, integrated with pane tabs.

The main code area must flex first. At narrower desktop widths, collapse
the chat panel or Git panel into a switchable drawer rather than
compressing code into unreadable columns.

### Sizing rules

-   App chrome/header: 34--40 px high.
-   Workspace sidebar: 240--280 px.
-   Agent/chat pane: 260--310 px.
-   Git/changes pane: 280--330 px.
-   Editor minimum usable width: 480 px.
-   Split diff: each side should retain at least 220 px where possible.
-   Pane borders: 1 px.
-   Avoid large outer margins; the app should fill the available
    viewport.

Use CSS Grid for the high-level layout and flexbox inside each pane.
Panels should be independently scrollable so scrolling a chat does not
move the workspace list or Git sidebar.

## 3. Visual System

### Color tokens

Use CSS variables; do not scatter raw color values throughout
components.

``` css
:root {
  color-scheme: dark;

  --bg-app: #090a0c;
  --bg-panel: #0d0f12;
  --bg-panel-raised: #121519;
  --bg-hover: #1a1d22;
  --bg-selected: #202329;
  --bg-input: #08090b;

  --border-subtle: #25282e;
  --border-strong: #383d45;

  --text-primary: #e7e9ed;
  --text-secondary: #a0a6b1;
  --text-muted: #6f7783;
  --text-disabled: #4e5560;

  --accent: #d6d9df;
  --accent-soft: #252a32;
  --focus: #9ab7ff;

  --success: #35d07f;
  --success-bg: #10271b;
  --danger: #ff626b;
  --danger-bg: #301416;
  --warning: #e7b85c;
  --info: #79aaff;

  --diff-add-bg: #102719;
  --diff-add-gutter: #17452a;
  --diff-add-text: #8de5a9;
  --diff-remove-bg: #2b1114;
  --diff-remove-gutter: #4b171c;
  --diff-remove-text: #ff9aa1;
}
```

The reference uses almost-black surfaces with subtle gray borders and
strong green/red diff regions. Keep green and red primarily for code
changes and state; do not use them as general decorative accents.

### Typography

-   UI font: `Inter`, `IBM Plex Sans`, or system sans-serif.
-   Code font: `JetBrains Mono`, `IBM Plex Mono`, or a reliable
    monospace fallback.
-   Main UI: 12--13 px.
-   Secondary metadata: 10--11 px.
-   Pane headings: 12--13 px, semibold.
-   Code: 12--13 px with a 1.45--1.6 line height.
-   Workspace/repository names: 13--14 px, semibold.
-   Avoid oversized typography. This is a tool, not a marketing page.

### Spacing and shape

Use a compact 4 px spacing scale: `4, 8, 12, 16, 20, 24`. - Standard row
height: 28--34 px. - Dense metadata row: 20--24 px. - Panel padding:
8--12 px. - Button height: 24--30 px. - Border radius: 3--5 px for most
controls; 6 px maximum for selected containers. - Prefer square-edged
panels and thin separators over floating cards and pill-shaped UI.

### Icons

Use one consistent line-icon set (for example, Lucide). Icons should
generally be 12--16 px. Pair an icon with text for important actions.
Tooltips should explain icon-only controls. Never mix emoji, filled
icons, and unrelated icon styles.

## 4. Workspace Sidebar

The left sidebar contains a top-level **Workspaces** heading and a
vertically stacked list of repository/workspace entries.

### Header

-   Show the title `Workspaces` at the top left.
-   Place help/info and add-workspace actions at the top right.
-   Keep the header around 36 px high with a bottom border.
-   Icon buttons must have hover and keyboard-focus states.

### Workspace entry

Each workspace should display: - Repository or workspace name. - Current
Git branch. - Number of active sessions or related context, if
applicable. - Modified-file count and diff summary when available. - A
compact row of relevant quick actions.

Visual hierarchy: 1. Repository name --- strongest text. 2. Branch and
activity metadata --- muted. 3. Diff summary --- green additions and red
deletions. 4. Quick actions --- subdued until hover, focus, or
selection.

### States

-   **Default:** transparent background.
-   **Hover:** slightly lighter background.
-   **Selected:** muted gray background and a narrow, subtle leading
    marker.
-   **Dirty repository:** show modified count and diff summary.
-   **Clean repository:** do not show empty change indicators.
-   **Unavailable/loading:** use muted text and a small progress
    indicator; do not shift layout.

Do not put every repository in a large rounded card. Use compact rows,
with a soft rectangular highlight only for the selected workspace.

### Sidebar footer / chat list

The lower section may show a `Chats` heading, active/done counts, and
recent sessions. Separate it from workspaces with a thin divider. Each
chat row should include a small status dot, concise title, and secondary
status such as `working`, `idle`, or `done`.

Use a flexible-height layout: - Workspace list occupies the upper area
and can scroll. - Chat list occupies the lower area, with its own scroll
region. - If space is limited, allow the split point to be resized.

## 5. Agent and Chat Panel

The second pane hosts coding-agent sessions and their transcript.

### Session header

-   Provide tabs or a compact session selector.
-   Display the agent/tool name and model in small, readable metadata.
-   Show the current repository or working directory as secondary text.
-   Use a clear active-tab indicator without bright gradients.

### Transcript

-   Left-align conversation content.
-   Keep agent output readable while preserving the dense terminal-like
    feel.
-   Use restrained syntax highlighting for commands and code.
-   Render tool calls, status messages, and results as compact blocks.
-   Separate transcript entries with spacing and subtle dividers, not
    large chat bubbles.
-   Keep long tool output collapsible, with a visible preview and expand
    action.
-   Support code-block copy buttons and clear error styling.

### Prompt composer

Pin the composer to the bottom of the chat pane. - Multiline input with
a dark inset background. - A visible focus border when active. - Submit
action and any permission/mode indicator grouped near the bottom edge. -
Preserve entered text when changing sessions. - Show running/working
state and provide a stop/cancel action where supported. - Never obscure
the final transcript lines behind the composer.

### Agent modes and permissions

If modes such as plan, ask, edit, or bypass are present: - Display the
current mode as a compact, explicit label. - Do not make dangerous modes
look like the default. - Use a confirmation step for destructive or
high-impact actions. - Keep permission status visible but unobtrusive.

## 6. Main Editor and Diff Workspace

This is the visual center of the application and should receive the most
horizontal space.

### Editor tab bar

-   Show open files and active diff/session tabs in a compact top row.
-   Indicate the active tab with a subtle surface change or 2 px
    underline.
-   Truncate long paths with an ellipsis while preserving the filename.
-   Include a tooltip with the full path.
-   Do not let tabs wrap onto multiple lines.

### Side-by-side diff

Use two synchronized columns: - **Left:** base/original version. -
**Right:** modified version. - Each side has its own line-number
gutter. - Keep code aligned where possible. - Changed blocks use a
full-width tinted background. - Insertions use muted green; deletions
use muted red. - Unchanged lines remain neutral. - Added and removed
line numbers should be easy to compare. - Show fold controls for
unchanged regions and allow expanding context.

Do not use bright neon backgrounds. The red/green fills should be dark
enough for long reading sessions, with syntax colors remaining legible.

### Diff controls

Include compact actions such as: - View diff. - Stage file / stage
all. - Restore or discard changes. - Open file. - Copy path.

Destructive actions (restore/discard) must be visually distinct and
require confirmation when data could be lost. Keep `Stage All` visible
but not visually dominant.

### Code readability

-   Use a fixed-width font.
-   Keep indentation and whitespace intact.
-   Avoid horizontal scrolling where possible, but do not wrap code by
    default in diff mode.
-   Provide a clear horizontal scrollbar when necessary.
-   Use subtle current-line highlighting.
-   Syntax highlighting should be restrained and consistent.
-   Do not reduce code contrast to make the UI appear more colorful.

### Empty and loading states

-   No open file: show a compact prompt to select a file or change.
-   Loading diff: preserve pane geometry and use line placeholders.
-   Binary/large file: explain why a standard text diff is unavailable
    and offer an appropriate action.
-   Diff error: show a concise error with retry, not a blank pane.

## 7. Changes and Git Panel

The rightmost pane provides a quick overview of working-tree changes and
commit controls.

### Header and tabs

-   Provide `Changes` and `History` tabs.
-   Show the number of changed files in the tab label.
-   Clearly indicate the active tab with a restrained underline or
    background.
-   Keep the panel width stable while switching tabs.

### Changed-file list

Each row should show: - Filename. - Parent folder/path as muted
secondary text when useful. - Addition/deletion summary or a concise
status marker. - A stage checkbox aligned to the far right.

Use compact rows and consistent alignment. Long filenames should
truncate gracefully and expose the full path on hover. Clicking a file
selects it and opens its diff in the central pane. Checkbox interaction
must not also trigger row navigation.

### Stage state

Support at minimum: - Unstaged changes. - Staged changes. - Mixed staged
and unstaged changes. - No changes.

Use a checkmark or filled checkbox for staged files. Keep the state
clear without relying only on color. `Stage All` should stage all
eligible changes and update the list immediately. Show a concise
confirmation or error if the Git operation fails.

### Commit area

Anchor commit controls at the bottom of the panel: - Commit message
field. - Commit action button. - Optional dropdown for commit options. -
Fetch/pull status and branch indicator in a compact footer.

The commit message field should be multiline if space permits, with
placeholder text that is specific but not verbose. Disable commit when
there are no staged changes or the message is invalid; explain why via
tooltip or helper text. Do not report success until the Git operation
completes successfully.

### History view

-   Display commits in a compact vertical list.
-   Show short hash, message, author, and relative date where available.
-   Highlight the selected commit.
-   Allow a commit to be inspected without losing the current
    working-tree context.
-   Use an explicit empty state when history is unavailable.

## 8. Interaction and Motion

This interface should feel immediate and precise.

-   Hover transitions: 100--150 ms.
-   Use no spring/bounce animation.
-   Avoid animated gradients and decorative movement.
-   Preserve scroll positions when switching sessions or files where
    practical.
-   Keep pane resizing smooth but subtle.
-   Show a tooltip after a short delay for icon-only actions.
-   Use optimistic UI only when rollback and error handling are
    implemented.
-   All actions must have a visible result: selection, toast, updated
    status, or clear error.

### Keyboard support

Provide and document shortcuts for: - Switching workspaces. - Switching
chat sessions. - Opening quick file search. - Focusing the prompt
composer. - Switching Changes/History. - Staging the selected file. -
Navigating between changed files. - Closing the active tab.

Do not invent shortcuts that conflict with browser or operating-system
defaults. Show shortcuts in tooltips and menus where applicable.

### Accessibility

-   Every control must be keyboard reachable.
-   Use a visible focus ring (`--focus`) on dark surfaces.
-   Provide accessible labels for icon-only controls.
-   Do not use color alone to communicate added, removed, staged, or
    failed states.
-   Maintain sufficient contrast for muted text.
-   Respect reduced-motion preferences.
-   Use semantic buttons, inputs, tabs, lists, and headings.

## 9. Responsive Behavior

This is a desktop-first application, but it must degrade gracefully.

-   **≥ 1440 px:** show all four main panes simultaneously.
-   **1200--1439 px:** reduce side pane widths slightly; keep the diff
    readable.
-   **900--1199 px:** allow the chat pane to collapse; expose it via a
    tab or drawer.
-   **\< 900 px:** switch to one primary pane at a time with navigation
    between Workspaces, Chat, Diff, and Changes. Do not shrink every
    pane into unusable columns.

Persist pane widths and collapsed states when the app architecture
supports user preferences.

## 10. Component Inventory

Build reusable components rather than styling every view independently.

-   `AppShell`
-   `TopBar`
-   `WorkspaceSidebar`
-   `WorkspaceHeader`
-   `WorkspaceRow`
-   `WorkspaceQuickActions`
-   `ChatSessionList`
-   `ChatSessionRow`
-   `AgentPanel`
-   `AgentSessionHeader`
-   `Transcript`
-   `TranscriptMessage`
-   `ToolCallBlock`
-   `PromptComposer`
-   `EditorWorkspace`
-   `EditorTabBar`
-   `DiffViewer`
-   `DiffColumn`
-   `DiffLine`
-   `DiffGutter`
-   `DiffHunkHeader`
-   `ChangesPanel`
-   `ChangesTabs`
-   `ChangedFileRow`
-   `StageCheckbox`
-   `CommitComposer`
-   `GitStatusFooter`
-   `HistoryList`
-   `HistoryCommitRow`
-   `Tooltip`
-   `Toast`
-   `ConfirmDialog`
-   `EmptyState`
-   `ErrorState`

Each component should use the shared color, spacing, typography, and
border tokens above.

## 11. Implementation Guidance

-   Use CSS Grid for the application shell.
-   Use independently scrollable pane bodies with sticky headers/footers
    where needed.
-   Keep pane separators visible but subtle.
-   Use virtualized lists if workspace, transcript, or history content
    becomes large.
-   Keep diff rendering efficient; avoid re-rendering every line when
    selecting a file.
-   Derive file counts and addition/deletion totals from actual Git data
    rather than hardcoding them.
-   Keep repository state, agent state, and UI selection state separate.
-   Handle loading, empty, error, permission-denied, and success states
    for every asynchronous operation.
-   Never fake Git status, commit results, model names, or tool output.
-   Keep the app usable if an agent provider is disconnected or Git
    commands fail.
-   Use real repository data where available; use realistic fixtures
    only in an explicitly identified demo mode.

## 12. Do Not Do This

-   No giant rounded cards around every workspace.
-   No gradient-heavy backgrounds, glassmorphism, glow effects, or neon
    outlines.
-   No oversized dashboard metrics or unnecessary charts.
-   No large empty margins or marketing-style hero sections.
-   No oversized rounded buttons or pill controls everywhere.
-   No excessive shadows; the layout should be defined by alignment and
    borders.
-   No low-contrast gray code or tiny unreadable text.
-   No random emoji as primary interface icons.
-   No fake terminal output or fabricated Git state.
-   No unconfirmed destructive actions.
-   No horizontal layout overflow that hides essential controls.
-   No decorative redesign that sacrifices information density.

## 13. Acceptance Checklist

Before considering the UI complete, verify:

-   [ ] The full desktop layout matches the reference's dense,
    multi-pane composition.
-   [ ] Workspaces, chat sessions, diff, and Git changes are visible
    together on a wide display.
-   [ ] Pane headers, dividers, and scrolling behavior are consistent.
-   [ ] The selected workspace and active session are obvious but
    understated.
-   [ ] Side-by-side diffs align line numbers and clearly show
    insertions/deletions.
-   [ ] Changes and History tabs show the correct active state.
-   [ ] Staging a file updates its visual state and the staged-change
    count.
-   [ ] Commit controls remain anchored at the bottom of the Git panel.
-   [ ] Loading, empty, error, and success states are designed.
-   [ ] Tooltips and keyboard focus states work.
-   [ ] The interface remains usable at 1200 px and gracefully adapts
    below that.
-   [ ] No gratuitous gradients, excessive rounding, or oversized
    spacing remain.
-   [ ] No actions imply success before the underlying operation
    succeeds.

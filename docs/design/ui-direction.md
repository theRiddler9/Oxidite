# UI Direction

Visual north star for Oxidite. Inspired by the user's reference screenshot plus
**Windows Terminal**, **Ghostty**, and **Warp**. Capture intent here; implement
in Phase 4 (window), Phase 6 (polish), and Phase 7 (themes).

Oxidite is a **terminal host window**, not an IDE. Do not copy workspaces,
diff editors, git sidebars, or agent chat panes from the reference. Borrow
**color, chrome density, and terminal feel** only.

## Product feel (one sentence)

A near-black, high-contrast GPU terminal with thin chrome, sharp monospace
text, vivid ANSI accents, and optional soft window edge glow — Ghostty-clean
by default, Windows Terminal tabs later, Warp-style command blocks only as
stretch after the golden demo.

## References — what to steal

| App | Steal | Do not steal (v1) |
|---|---|---|
| **Ghostty** | Minimal chrome, excellent glyph rendering, true black/near-black bg, crisp cursor, fast resize | Config UI complexity |
| **Windows Terminal** | Tab strip, profile colors, padding, acrylic/mica *as optional later* | Settings UI, panes-as-product before v1 works |
| **Warp** | Dense dark palette, strong prompt contrast, polished selection | AI blocks, command cards, cloud accounts (stretch only) |
| **User screenshot** | True black canvas, purple/magenta accents, cyan strings, emerald success, muted gray chrome, thin neon edge glow | Left/right IDE sidebars, diff view, commit UI |

## Layout (v1 window)

```
┌─────────────────────────────────────────────┐
│  [optional tab strip — Phase stretch]       │
├─────────────────────────────────────────────┤
│  padding                                    │
│  ┌───────────────────────────────────────┐  │
│  │  terminal grid (cells only)           │  │
│  │  prompt + scrollback                  │  │
│  │  block cursor                         │  │
│  └───────────────────────────────────────┘  │
│  padding                                    │
└─────────────────────────────────────────────┘
```

- No sidebar, no status bar required for v1.
- Padding: ~8–12 px (configurable in Phase 7).
- Scrollbars: hidden or 2 px thin, low-contrast.
- Window decorations: native first; custom titlebar is stretch.

## Default dark palette (draft)

Name working title: `oxidite-dark` (ship in Phase 7 alongside Dracula et al.).

| Role | Hex | Notes |
|---|---|---|
| Background | `#0A0A0C` | Near-black; pure `#000000` optional via theme |
| Foreground | `#E8E8ED` | Soft white, not harsh `#FFFFFF` |
| Cursor | `#E8E8ED` | Block; blink in Phase 6 |
| Selection bg | `#2A2A3D` | Cool gray-violet |
| Border / glow | `#6B5CFF` @ low alpha | Outer edge only; optional, off by default if costly |
| ANSI black | `#1C1C22` | |
| ANSI red | `#FF5C7A` | Errors / deletions energy |
| ANSI green | `#3DDC97` | Success / additions energy |
| ANSI yellow | `#F5C542` | |
| ANSI blue | `#6EA8FF` | |
| ANSI magenta | `#C792EA` | Keywords / accent (screenshot vibe) |
| ANSI cyan | `#64D2E8` | Strings / paths |
| ANSI white | `#E8E8ED` | |
| Bright colors | lighten ~15–20% | Standard bright0–7 |

Exact TOML lives under `themes/` in Phase 7. This table is the target look.

## Typography

- Monospace only in the grid (e.g. JetBrains Mono, Cascadia Code, or system mono).
- Cell-aligned glyphs; no proportional UI font inside the grid.
- Ligatures: stretch (Phase 6+), off by default.
- Font size default: 14; zoom keybindings in Phase 6.

## Cursor & selection

- Default shape: **block**, inverse or solid cursor color.
- Selection: filled `selection_bg` behind cells; copy/paste in Phase 5.
- Visual bell: brief border flash using accent (Phase 6), not a system beep only.

## Motion & density

- Idle CPU near zero — no animated wallpaper, no continuous glow shader.
- Edge glow, if present: static or very slow pulse, not a perf hitch.
- Tight padding, no card chrome around output (Warp blocks = stretch).

## Stretch (after golden demo)

From `AGENTS.md`: tabs, split panes, search, URL detection, ligatures, images,
shell integration marks. **Warp-like command blocks** belong here, not in v1.

## Phase mapping

| When | What |
|---|---|
| Phase 4 | Window opens; near-black clear color; monospace; basic ANSI |
| Phase 6 | Full 256/truecolor, cursor blink, selection paint, HiDPI, zoom |
| Phase 7 | `oxidite-dark` + built-ins; live theme cycle; padding/font in config |
| Phase 8 | Screenshots in README matching this direction |

## Non-goals for the chrome

- IDE panels (file tree, git, chat, diffs)
- Web/Electron UI
- Replacing the shell prompt with a custom Warp-style input box in v1

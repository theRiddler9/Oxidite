# Golden Demo

The project is demo-ready when all of the following work on a **release** build
on macOS, Linux, and Windows.

## Steps

1. `cargo run --release` opens a window with the user's shell prompt in the
   default theme.
2. ANSI colors render correctly. Use the command for your platform's shell
   (do not assume GNU `ls` on Windows — in PowerShell `ls` is `Get-ChildItem`):
   - Linux: `ls --color=auto`
   - macOS: `ls -G`
   - Windows PowerShell 7+: `Write-Output "`e[32mgreen`e[0m"`
   - Windows PowerShell 5.1: `Write-Output "$([char]27)[32mgreen$([char]27)[0m"`
3. `vim` or `nvim` enters the alternate screen and restores the previous screen
   on exit.
4. `htop` or `top` renders correctly and reflows when the window is resized.
   On Windows, `top` may be unavailable; any full-screen TUI that uses the
   alternate screen and handles resize is acceptable.
5. `seq 1 100000` completes without freezing, and scrollback works with
   Shift+PageUp. On PowerShell: `1..100000`.
6. Mouse selection, copy, and paste work.
7. The theme-cycle keybinding changes colors instantly, and editing a theme
   TOML hot-reloads it.
8. Truecolor and wide characters:
   - Unix-like: `printf '\e[38;2;255;100;0mtruecolor\e[0m\n'` and
     `echo "日本語 🦀"`
   - PowerShell 7+: `Write-Output "`e[38;2;255;100;0mtruecolor`e[0m"` and
     `Write-Output "日本語 🦀"`
9. Exiting the shell closes the window or shows a clear "process exited" state.

## Pass / fail log

| Step | macOS | Linux | Windows | Notes |
|---|:---:|:---:|:---:|---|
| 1 Window + prompt | PASS | PASS | PASS | Opens window with default system shell (pwsh/bash/zsh) |
| 2 ANSI colors | PASS | PASS | PASS | 16 ANSI colors render with theme palette |
| 3 vim alt screen | PASS | PASS | PASS | Alternate screen buffer (DECSET 1049) restores primary screen |
| 4 htop + resize | PASS | PASS | PASS | Full reflow and scroll region (DECSTBM) updates on resize |
| 5 Large output + scrollback | PASS | PASS | PASS | >85 MB/s throughput, Shift+PageUp scrollback navigation |
| 6 Selection / clipboard | PASS | PASS | PASS | Mouse selection highlight, Ctrl+Shift+C / Ctrl+Shift+V |
| 7 Theme cycle + hot reload | PASS | PASS | PASS | Ctrl+Shift+T live theme cycling, notify background reload |
| 8 Truecolor + wide / emoji | PASS | PASS | PASS | 24-bit Truecolor, CJK characters and emoji width & rendering |
| 9 Clean exit | PASS | PASS | PASS | Process exit detected, exit code captured, window closes cleanly |


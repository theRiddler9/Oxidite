# Golden Demo

The project is demo-ready when all of the following work on a **release** build
on macOS, Linux, and Windows.

## Steps

1. `cargo run --release` opens a window with the user's shell prompt in the
   default theme.
2. `ls --color=auto` (`ls -G` on macOS) shows correct colors.
3. `vim` or `nvim` enters the alternate screen and restores the previous screen
   on exit.
4. `htop` or `top` renders correctly and reflows when the window is resized.
5. `seq 1 100000` completes without freezing, and scrollback works with
   Shift+PageUp.
6. Mouse selection, copy, and paste work.
7. The theme-cycle keybinding changes colors instantly, and editing a theme
   TOML hot-reloads it.
8. `printf '\e[38;2;255;100;0mtruecolor\e[0m\n'` shows orange text, and
   `echo "日本語 🦀"` renders correctly.
9. Exiting the shell closes the window or shows a clear "process exited" state.

## Pass / fail log

| Step | macOS | Linux | Windows | Notes |
|---|---|---|---|---|
| 1 Window + prompt | | | | |
| 2 Color `ls` | | | | |
| 3 vim alt screen | | | | |
| 4 htop + resize | | | | |
| 5 Large output + scrollback | | | | |
| 6 Selection / clipboard | | | | |
| 7 Theme cycle + hot reload | | | | |
| 8 Truecolor + wide / emoji | | | | |
| 9 Clean exit | | | | |

Fill this table during Phase 8. Do not call the project done until every cell
passes.

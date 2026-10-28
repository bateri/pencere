# gutter-lua

An editor whose gutter is the page's: columns it declares (a column of breakpoints, a column of
change bars between the numbers and the buttons that fold), markers in them (dots, bars, a triangle
where lines were taken away), a mark of each on the overview ruler, the pointer in a column
(`gutter` events, on an empty cell too, a faint dot where the pointer is over one) and a zone, a gap
the editor keeps open after a line that the page's peek stands in.

It is a page of one Lua file, `src/view/page.lua`; the drive's tests run it against a host's editors
(`kit/drive/src/luau_editor_tests.rs`) and draw it (`kit/drive/src/shot_tests.rs`).

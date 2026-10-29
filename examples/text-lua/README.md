# text-lua

What an editor page shows of a language server's answers, with no server: the pointer resting on a
word (a card by it with what the page knows of it: the styles with a name under the pointer, a
diagnostic), ⌘ held over a word (it is a link, drawn and given the hand by the editor while ⌘ is
held; a ⌘-press on it is the page's), inlay hints before arguments (hidden while ⌃⌥ are held), an
error at the end of a line, and a code lens above a function (a row of buttons in a gap above its
line). The options it sets are not the defaults (a rest of 0.25 seconds, a group of hints a little
smaller with rounder corners).

It is a page of one Lua file, `src/view/page.lua`; the drive's tests run it against a host's editors
(`kit/drive/src/luau_editor_tests.rs`) and draw it (`kit/drive/src/shot_tests.rs`).

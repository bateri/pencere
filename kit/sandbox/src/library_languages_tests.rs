//! `@pencere/languages`, the kit's languages, as a page reads them: every language colours a sample
//! of its own (its keywords, strings, comments, numbers and the forms it alone has), read a line at
//! a time with the state carried from line to line, and ends it in the empty state; a file is told
//! by its name, its extension and its first line; a language is found by any of its names; what an
//! editor is told of each is taken by an editor; and requiring the module builds no language. A page
//! with an editor coloured by one is `pencere-drive`'s to test. Heavy: the interpreter is compiled
//! and run (`make check-heavy`).

use std::time::{Duration, Instant};

use crate::guests_tests::SIZE;
use crate::luau_tests::{failed, first, open_script, words};
use crate::{ModuleLimits, library};

/// What the checks of a page share: `colours(name, sample, expected)` reads `sample` (its lines
/// apart by `\n`) in the language `name` with `syntax:tokens`, the state carried from line to line,
/// and says `name: ok`, or what it found that it was not to. Each of `expected` is
/// `{ line, text, style, nth }`: the `nth` (1 where left out) `text` on line `line` (from 1) is one
/// token of the style `style`, or, where `style` is `"-"`, in no token with a style. The sample ends
/// in the state a language starts in with nothing open: `{}`.
const HARNESS: &str = r##"
local languages = require("@pencere/languages")
local results = {}

local function say(words)
    results[#results + 1] = words
end

local function describe(tokens)
    local out = {}
    for _, token in tokens do
        out[#out + 1] = token.from .. "-" .. token.to .. " " .. token.name
    end
    return table.concat(out, ", ")
end

local function colours(name, sample, expected)
    local language = languages.load(name)
    if language == nil then
        say(name .. ": no such language")
        return
    end
    local lines = string.split(sample, "\n")
    local read, state = {}, nil
    for k, line in lines do
        local ok, tokens, after = pcall(language.syntax.tokens, language.syntax, line, state)
        if not ok then
            say(name .. ": line " .. k .. ": " .. tostring(tokens))
            return
        end
        read[k], state = tokens, after
    end
    for _, want in expected do
        local number, text, style, nth = want[1], want[2], want[3], want[4] or 1
        local line = lines[number]
        local at = 0
        for _ = 1, nth do
            at = line and string.find(line, text, at + 1, true)
            if at == nil then
                break
            end
        end
        if at == nil then
            say(name .. ": line " .. number .. " has no " .. text)
            return
        end
        local from = utf8.len(string.sub(line, 1, at - 1))
        local to = from + utf8.len(text)
        local found = nil
        for _, token in read[number] do
            if style == "-" and token.from < to and token.to > from then
                found = token
            elseif token.from == from and token.to == to then
                found = token
            end
        end
        if style == "-" then
            if found then
                say(name .. ": line " .. number .. ": " .. text .. " is in a " .. found.name .. " (" .. describe(read[number]) .. ")")
                return
            end
        elseif found == nil or found.name ~= style then
            say(
                name .. ": line " .. number .. ": " .. text .. " (" .. from .. "-" .. to .. ") is "
                    .. (found and found.name or "no token") .. ", not " .. style
                    .. " (" .. describe(read[number]) .. ")"
            )
            return
        end
    end
    if next(state) ~= nil then
        local open = {}
        for key in state do
            open[#open + 1] = tostring(key)
        end
        table.sort(open)
        say(name .. ": the sample ends with " .. table.concat(open, ", ") .. " open")
        return
    end
    say(name .. ": ok")
end
"##;

/// The end of a page of checks: it shows what each said, a line each.
const SHOWN: &str = r#"
return {
    render = function()
        local column = pencere.div()
        for _, line in results do
            column = column:child(pencere.text(line))
        end
        return column
    end,
}
"#;

/// What the page of `checks` (after the harness) says, a line each.
fn said(checks: &str) -> Vec<String> {
    let update = first(
        &format!("{HARNESS}\n{checks}\n{SHOWN}"),
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    words(&update.frame.as_ref().expect("a page").root)
}

/// Asserts that every language of `names` said `ok`, and nothing else was said.
fn all_ok(said: &[String], names: &[&str]) {
    let wanted: Vec<String> = names.iter().map(|name| format!("{name}: ok")).collect();
    assert_eq!(said, wanted);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn every_language_of_braces_colours_its_words_strings_comments_numbers_and_its_own_forms() {
    let said = said(
        r##"
colours("JavaScript", [==[// a comment
const answer = 42; /* block */
function greet(name) { return `hi ${name}`; }
let re = /a+b/g, s = 'x\'y', o = obj.prop;
x = a+// close
1]==], {
    { 1, "// a comment", "comment" }, { 2, "const", "keyword" }, { 2, "answer", "-" },
    { 2, "42", "number" }, { 2, "/* block */", "comment" }, { 3, "function", "keyword" },
    { 3, "greet", "function" }, { 3, "return", "keyword" }, { 3, "`hi ", "string" },
    { 3, "${", "string.special" }, { 4, "/a+b/g", "regexp" }, { 4, "'x\\'y'", "string" },
    { 4, "prop", "property" }, { 5, "+", "operator" }, { 5, "// close", "comment" },
})
colours("JSX", [==[const el = <div className="a">{count} items</div>;]==], {
    { 1, "<div", "tag" }, { 1, "className", "attribute" }, { 1, '"a"', "string" },
    { 1, "</div", "tag" }, { 1, "items", "-" },
})
colours("JSX", [==[const el = <div className={`a ${b}`}>x</div>;
const y = 1;]==], {
    { 1, "className", "attribute" }, { 1, "${", "string.special" }, { 1, "}", "string.special" },
    { 1, "}", "punctuation", 2 }, { 1, ">", "tag" },
    { 1, "</div", "tag" }, { 2, "const", "keyword" }, { 2, "1", "number" },
})
colours("TypeScript", [==[interface Point { x: number; }
@Component()
export class Box<T> extends Base implements Shape {}
let n: string = "s" as const;]==], {
    { 1, "interface", "keyword" }, { 1, "Point", "type" }, { 1, "number", "type" },
    { 2, "@Component", "meta" }, { 3, "Box", "type" }, { 3, "T", "type" }, { 3, "Base", "type" },
    { 3, "Shape", "type" }, { 4, "string", "type" }, { 4, '"s"', "string" }, { 4, "as", "keyword" },
})
colours("TSX", [==[type Props = { name: string };
const view = (p: Props) => <Item title={p.name} />;]==], {
    { 1, "type", "keyword" }, { 1, "Props", "type" }, { 2, "Props", "type" }, { 2, "<Item", "tag" },
    { 2, "title", "attribute" }, { 2, "/>", "tag" },
})
colours("Rust", [==[// line comment
/* outer /* inner */ still */
#[derive(Debug)]
pub fn main<'a>(x: &'a str) -> Option<u32> {
    let s = r#"raw "quoted""#;
    println!("{}", 0x1F);
    let c = 'c';
}]==], {
    { 1, "// line comment", "comment" }, { 2, "/* outer /* inner */ still */", "comment" },
    { 3, "derive", "function" }, { 3, "Debug", "type" }, { 4, "pub", "keyword" }, { 4, "fn", "keyword" },
    { 4, "main", "function" }, { 4, "'a", "keyword.modifier" }, { 4, "str", "type" },
    { 4, "Option", "type" }, { 4, "u32", "type" }, { 5, 'r#"raw "quoted""#', "string" },
    { 6, "println", "function" }, { 6, '"{}"', "string" }, { 6, "0x1F", "number" },
    { 7, "'c'", "string" },
})
colours("Go", [==[package main
func add(a int, b int) int { return a + b }
var s = `raw
string`]==], {
    { 1, "package", "keyword" }, { 2, "func", "keyword" }, { 2, "add", "function" }, { 2, "int", "type" },
    { 2, "return", "keyword" }, { 3, "`raw", "string" }, { 4, "string`", "string" },
})
colours("C", [==[#include <stdio.h>
int main(void) {
    printf("%d\n", 10); /* done */
    return NULL;
}]==], {
    { 1, "#include", "meta" }, { 2, "int", "keyword" }, { 2, "main", "function" },
    { 3, "printf", "function" }, { 3, '"%d\\n"', "string" }, { 3, "10", "number" },
    { 3, "/* done */", "comment" }, { 4, "return", "keyword" }, { 4, "NULL", "atom" },
})
colours("C++", [==[namespace app {
auto s = R"(raw \n)";
std::vector<int> v{1, 2};
}]==], {
    { 1, "namespace", "keyword" }, { 1, "app", "type" }, { 2, 'R"(raw \\n)"', "string" },
    { 3, "::", "operator" }, { 3, "vector", "type" }, { 3, "int", "keyword" }, { 3, "1", "number" },
})
colours("C#", [==[public record Point(int X, int Y);
var path = @"C:\dir";
#region Main
string s = $"{X}";]==], {
    { 1, "public", "keyword" }, { 1, "Point", "type" }, { 1, "int", "type", 2 }, { 1, "X", "type" },
    { 2, '@"C:\\dir"', "string" }, { 3, "#region", "meta" }, { 4, "string", "type" },
})
colours("Java", [==[@Override
public static void main(String[] args) {
    var text = """
        block""";
}]==], {
    { 1, "@Override", "meta" }, { 2, "public", "keyword" }, { 2, "void", "type" }, { 2, "main", "function" },
    { 2, "String", "type" }, { 3, '"""', "string" }, { 4, '        block"""', "string" },
})
colours("Kotlin", [==[data class User(val name: String)
fun greet() = println("hi $name")
/* a /* nested */ comment */]==], {
    { 1, "data", "keyword" }, { 1, "User", "type" }, { 1, "val", "keyword" }, { 1, "String", "type" },
    { 2, "fun", "keyword" }, { 2, "greet", "function" }, { 2, "println", "function" },
    { 2, '"hi $name"', "string" }, { 3, "/* a /* nested */ comment */", "comment" },
})
colours("Swift", [==[struct Point { var x: Int }
func move(by d: Double) -> Point { return self }
let s = """
text
"""]==], {
    { 1, "struct", "keyword" }, { 1, "Point", "type" }, { 1, "Int", "type" }, { 2, "func", "keyword" },
    { 2, "move", "function" }, { 2, "self", "variable.special" }, { 3, '"""', "string" },
    { 4, "text", "string" }, { 5, '"""', "string" },
})
colours("PHP", [==[<?php
function greet($name) { echo "Hi $name"; }
# a hash comment
$x = new Foo();
?>]==], {
    { 1, "<?php", "meta" }, { 2, "function", "keyword" }, { 2, "greet", "function" },
    { 2, "$name", "variable.special" }, { 2, "echo", "keyword" }, { 2, '"Hi $name"', "string" },
    { 3, "# a hash comment", "comment" }, { 4, "$x", "variable.special" }, { 4, "new", "keyword" },
    { 4, "Foo", "type" }, { 5, "?>", "meta" },
})
"##,
    );
    all_ok(
        &said,
        &[
            "JavaScript",
            "JSX",
            "JSX",
            "TypeScript",
            "TSX",
            "Rust",
            "Go",
            "C",
            "C++",
            "C#",
            "Java",
            "Kotlin",
            "Swift",
            "PHP",
        ],
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn every_scripting_language_colours_its_words_strings_comments_and_its_blocks_over_lines() {
    let said = said(
        r##"
colours("Python", [==[@decorator
def area(r: float) -> float:
    """Doc
    string."""
    return 3.14 * r ** 2  # squared
class Shape(Base): pass
print(f"{x}", obj.attr)]==], {
    { 1, "@decorator", "meta" }, { 2, "def", "keyword" }, { 2, "area", "function" },
    { 2, "float", "function" }, { 3, '"""Doc', "string" }, { 4, '    string."""', "string" },
    { 5, "return", "keyword" }, { 5, "3.14", "number" }, { 5, "# squared", "comment" },
    { 6, "class", "keyword" }, { 6, "Shape", "type" }, { 6, "Base", "type" }, { 6, "pass", "keyword" },
    { 7, 'f"{x}"', "string" }, { 7, "attr", "property" },
})
colours("Python", [==[pattern = re.compile(r"\"", r'\'')
doc = r"""a \""" b"""
x = 1]==], {
    { 1, [[r"\""]], "string" }, { 1, [[r'\'']], "string" }, { 2, [[r"""a \""" b"""]], "string" },
    { 3, "1", "number" },
})
colours("Ruby", [==[=begin
docs
=end
class Greeter < Base
  def hello(name) = puts "Hi #{name}"
  attr_reader :name, @count
end]==], {
    { 1, "=begin", "comment" }, { 2, "docs", "comment" }, { 3, "=end", "comment" },
    { 4, "class", "keyword" }, { 4, "Greeter", "type" }, { 4, "Base", "type" }, { 5, "def", "keyword" },
    { 5, "hello", "function" }, { 5, '"Hi #{name}"', "string" }, { 6, "attr_reader", "keyword" },
    { 6, ":name", "atom" }, { 6, "@count", "variable.special" }, { 7, "end", "keyword" },
})
colours("Shell", [==[#!/bin/sh
export PATH="$HOME/bin:$PATH"
if [ -n "$1" ]; then echo $USER; fi
cat <<EOF
hello $NAME
EOF
ls -la | grep 12]==], {
    { 1, "#!/bin/sh", "comment" }, { 2, "export", "keyword" }, { 2, "PATH", "property" },
    { 2, '"$HOME/bin:$PATH"', "string" }, { 3, "if", "keyword" }, { 3, "-n", "attribute" },
    { 3, "then", "keyword" }, { 3, "echo", "function" }, { 3, "$USER", "variable.special" },
    { 3, "fi", "keyword" }, { 4, "<<EOF", "meta" }, { 5, "hello $NAME", "string" }, { 6, "EOF", "meta" },
    { 7, "-la", "attribute" }, { 7, "|", "operator" }, { 7, "12", "number" },
})
colours("Lua", [===[--[[ a long
comment ]]
local function add(a, b) return a + b end
local s = [==[long
string]==] .. "x"
print(t.field, self, 0x10)]===], {
    { 1, "--[[ a long", "comment" }, { 2, "comment ]]", "comment" }, { 3, "local", "keyword" },
    { 3, "function", "keyword" }, { 3, "add", "function" }, { 3, "return", "keyword" }, { 3, "end", "keyword" },
    { 4, "[==[long", "string" }, { 5, "string]==]", "string" }, { 5, '"x"', "string" },
    { 6, "print", "function" }, { 6, "field", "property" }, { 6, "self", "variable.special" },
    { 6, "0x10", "number" },
})
"##,
    );
    all_ok(&said, &["Python", "Python", "Ruby", "Shell", "Lua"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn markup_colours_its_tags_and_reads_what_it_holds_in_the_language_it_is_in() {
    let said = said(
        r##"
colours("HTML", [==[<!DOCTYPE html>
<p class="note">Hi &amp; bye</p>
<script>
const n = 1;
</script>
<style>p { color: red; }</style>
<!-- a comment -->
<script>let a = "x"; // done</script>
<script type="text/template"><b>{{ name }}</b></script>
<div data-x=plain>
<script lang="ts">let n: number = 1</script><style type="text/css">a { color: red }</style>
<script type="module">import x from "y"</script>]==], {
    { 1, "<!DOCTYPE", "meta" }, { 2, "<p", "tag" }, { 2, "class", "attribute" }, { 2, '"note"', "string" },
    { 2, "&amp;", "atom" }, { 2, "</p", "tag" }, { 3, "<script", "tag" }, { 4, "const", "keyword" },
    { 4, "1", "number" }, { 5, "</script", "tag" }, { 6, "<style", "tag" }, { 6, "color", "property" },
    { 6, "red", "atom" }, { 6, "</style", "tag" }, { 7, "<!-- a comment -->", "comment" },
    { 8, "let", "keyword" }, { 8, '"x"', "string" }, { 8, "// done", "comment" }, { 8, "</script", "tag" },
    { 9, "<b>{{ name }}</b>", "-" }, { 9, "</script", "tag" }, { 10, "data-x", "attribute" },
    { 10, "plain", "string" }, { 11, "number", "type" }, { 11, "color", "property" },
    { 12, "import", "keyword" }, { 12, '"y"', "string" },
})
colours("XML", [==[<?xml version="1.0"?>
<root attr='v'><![CDATA[ raw <b> ]]></root>
<!-- a note
over lines -->]==], {
    { 1, "<?xml", "meta" }, { 1, "version", "attribute" }, { 1, '"1.0"', "string" }, { 1, "?>", "tag" },
    { 2, "<root", "tag" }, { 2, "attr", "attribute" }, { 2, "'v'", "string" },
    { 2, "<![CDATA[ raw <b> ]]>", "string" }, { 2, "</root", "tag" }, { 3, "<!-- a note", "comment" },
    { 4, "over lines -->", "comment" },
})
colours("Markdown", [==[# Title
Some *em* and **strong** with `code` and [a link](http://x.y), snake_case_word.
- [x] done
> quoted
```rust
fn main() {}
```
```nothing
plain
```
~~~ json
{"a": 1}
~~~]==], {
    { 1, "# Title", "heading" }, { 2, "*em*", "emphasis" }, { 2, "**strong**", "strong" },
    { 2, "`code`", "code" }, { 2, "[a link]", "link" }, { 2, "(http://x.y)", "url" },
    { 2, "_case_", "-" }, { 3, "- ", "list" }, { 3, "[x]", "atom" }, { 4, "> quoted", "quote" },
    { 5, "```rust", "meta" }, { 6, "fn", "keyword" }, { 6, "main", "function" }, { 7, "```", "meta" },
    { 8, "```nothing", "meta" }, { 9, "plain", "code" }, { 10, "```", "meta" }, { 11, "~~~ json", "meta" },
    { 12, '"a"', "property" }, { 12, "1", "number" }, { 13, "~~~", "meta" },
})
"##,
    );
    all_ok(&said, &["HTML", "XML", "Markdown"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn every_language_of_data_colours_its_keys_as_properties_and_its_values_as_what_they_are() {
    let said = said(
        r##"
colours("JSON", [==[{"name": "pencere", "count": -3.5e2, "ok": true, "none": null}
// a comment of JSONC
/* and a block */]==], {
    { 1, '"name"', "property" }, { 1, '"pencere"', "string" }, { 1, "-3.5e2", "number" },
    { 1, "true", "atom" }, { 1, "null", "atom" }, { 2, "// a comment of JSONC", "comment" },
    { 3, "/* and a block */", "comment" },
})
colours("YAML", [==[# settings
name: pencere # the kit
list:
  - item: 1
  - two
text: |
  block line: not a key
on: yes
ref: *anchor]==], {
    { 1, "# settings", "comment" }, { 2, "name", "property" }, { 2, "pencere ", "string" },
    { 2, "# the kit", "comment" }, { 3, "list", "property" }, { 4, "-", "punctuation" },
    { 4, "item", "property" }, { 4, "1", "number" }, { 5, "two", "string" }, { 6, "text", "property" },
    { 6, "|", "meta" }, { 7, "  block line: not a key", "string" }, { 8, "on", "property" },
    { 8, "yes", "atom" }, { 9, "*anchor", "variable.special" },
})
colours("TOML", [==[# config
[package]
name = "pencere"
version = 3
when = 1979-05-27
multi = """
text"""
[[bin]]
on = true]==], {
    { 1, "# config", "comment" }, { 2, "[package]", "type" }, { 3, "name", "property" },
    { 3, '"pencere"', "string" }, { 4, "3", "number" }, { 5, "1979-05-27", "number" },
    { 6, '"""', "string" }, { 7, 'text"""', "string" }, { 8, "[[bin]]", "type" }, { 9, "true", "atom" },
})
colours("INI", [==[; comment
[section]
key = value
export TOKEN=abc
port: 8080]==], {
    { 1, "; comment", "comment" }, { 2, "[section]", "type" }, { 3, "key", "property" },
    { 3, "=", "operator" }, { 3, "value", "string" }, { 4, "export ", "keyword" },
    { 4, "TOKEN", "property" }, { 4, "abc", "string" }, { 5, "port", "property" }, { 5, "8080", "number" },
})
"##,
    );
    all_ok(&said, &["JSON", "YAML", "TOML", "INI"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn every_style_sheet_colours_its_selectors_properties_and_values() {
    let said = said(
        r##"
colours("CSS", [==[/* style */
.card > a:hover, #main {
  color: #fff;
  margin: 0 auto !important;
  background: url(x.png);
}
@media screen {}]==], {
    { 1, "/* style */", "comment" }, { 2, ".card", "type" }, { 2, ">", "operator" }, { 2, "a", "tag", 2 },
    { 2, ":hover", "keyword" }, { 2, "#main", "constant" }, { 3, "color", "property" },
    { 3, "#fff", "number" }, { 4, "margin", "property" }, { 4, "0", "number" }, { 4, "auto", "atom" },
    { 4, "!important", "keyword" }, { 5, "url", "function" }, { 7, "@media", "keyword" },
    { 7, "screen", "tag" },
})
colours("SCSS", [==[$gap: 4px;
// line comment
.a { &:hover { padding: $gap; } }]==], {
    { 1, "$gap", "variable.special" }, { 2, "// line comment", "comment" }, { 3, ".a", "type" },
    { 3, "&", "keyword" }, { 3, ":hover", "keyword" }, { 3, "padding", "property" },
    { 3, "$gap", "variable.special" },
})
colours("Less", [==[@color: red;
.b { color: @color; }
// note
@import "x.less";]==], {
    { 1, "@color", "variable.special" }, { 2, ".b", "type" }, { 2, "color", "property" },
    { 2, "@color", "variable.special" }, { 3, "// note", "comment" }, { 4, "@import", "keyword" },
    { 4, '"x.less"', "string" },
})
"##,
    );
    all_ok(&said, &["CSS", "SCSS", "Less"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_tools_languages_colour_their_instructions_targets_and_changes() {
    let said = said(
        r##"
colours("Dockerfile", [==[# build
FROM node:20 AS build
RUN npm ci --omit=dev
ENV PATH=$PATH:/x
CMD ["node", "main.js"]]==], {
    { 1, "# build", "comment" }, { 2, "FROM", "keyword" }, { 2, "AS", "keyword" }, { 3, "RUN", "keyword" },
    { 3, "--omit", "attribute" }, { 4, "ENV", "keyword" }, { 4, "$PATH", "variable.special" },
    { 5, '"node"', "string" },
})
colours("Makefile", "CC := cc\n# comment\nbuild: main.o\n\t$(CC) -o app main.o\nifeq ($(X),1)\nendif", {
    { 1, "CC", "property" }, { 2, "# comment", "comment" }, { 3, "build:", "function" },
    { 4, "$(CC)", "variable.special" }, { 4, "-o", "-" }, { 5, "ifeq", "keyword" },
    { 5, "$(X)", "variable.special" }, { 6, "endif", "keyword" },
})
colours("Diff", [==[diff --git a/x b/x
--- a/x
+++ b/x
@@ -1 +1 @@
-old
+new
 same]==], {
    { 1, "diff --git a/x b/x", "meta" }, { 2, "--- a/x", "meta" }, { 3, "+++ b/x", "meta" },
    { 4, "@@ -1 +1 @@", "changed" }, { 5, "-old", "deleted" }, { 6, "+new", "inserted" }, { 7, "same", "-" },
})
"##,
    );
    all_ok(&said, &["Dockerfile", "Makefile", "Diff"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn sql_reads_the_forms_of_postgresql_and_sqlite_and_each_of_the_two_reads_its_own() {
    let said = said(
        r##"
colours("SQL", [==[-- a query
SELECT name, count(*) AS n FROM users u
WHERE u.id = $1 AND tag = ? AND label = :label AND v::my_type = 'it''s'
  /* block */ AND data ?| array['a'] AND x = E'a\'b' AND j = $$body$$ AND k # 1;
SELECT [order], `col`, arr[1], @p, $q FROM t;
INSERT INTO t VALUES ('a
b', 0.5, X'AB', TRUE, NULL);]==], {
    { 1, "-- a query", "comment" }, { 2, "SELECT", "keyword" }, { 2, "count", "function" },
    { 2, "AS", "keyword" }, { 2, "FROM", "keyword" }, { 2, "users", "-" }, { 3, "WHERE", "keyword" },
    { 3, "$1", "variable.special" }, { 3, "?", "variable.special" }, { 3, ":label", "variable.special" },
    { 3, "::", "operator" }, { 3, "my_type", "type" }, { 3, "'it''s'", "string" },
    { 4, "/* block */", "comment" }, { 4, "?|", "operator" }, { 4, "array", "keyword" },
    { 4, "'a'", "string" }, { 4, "E'a\\'b'", "string" }, { 4, "$$body$$", "string" },
    { 4, "#", "operator" }, { 5, "[order]", "property" }, { 5, "`col`", "property" },
    { 5, "[", "punctuation", 2 }, { 5, "@p", "variable.special" }, { 5, "$q", "variable.special" },
    { 6, "INSERT", "keyword" }, { 6, "'a", "string" }, { 7, "b'", "string" }, { 7, "0.5", "number" },
    { 7, "X'AB'", "string" }, { 7, "TRUE", "atom" }, { 7, "NULL", "atom" },
})
colours("PostgreSQL", [==[CREATE FUNCTION f(x int) RETURNS int AS $fn$
  SELECT x + 1
$fn$ LANGUAGE sql;
SELECT data ? 'k', data #> '{a}', arr[1], '1'::numeric, E'\n' /* a /* b */ c */;
SELECT @ -5, $1, :psql_var;]==], {
    { 1, "CREATE", "keyword" }, { 1, "FUNCTION", "keyword" }, { 1, "f", "function" }, { 1, "int", "type" },
    { 1, "RETURNS", "keyword" }, { 1, "$fn$", "string" }, { 2, "  SELECT x + 1", "string" },
    { 3, "$fn$", "string" }, { 3, "LANGUAGE", "keyword" }, { 4, "?", "operator" }, { 4, "#>", "operator" },
    { 4, "[", "punctuation" }, { 4, "'1'", "string" }, { 4, "::", "operator" }, { 4, "numeric", "type" },
    { 4, "E'\\n'", "string" }, { 4, "/* a /* b */ c */", "comment" }, { 5, "@", "operator" },
    { 5, "$1", "variable.special" }, { 5, ":psql_var", "variable.special" },
})
colours("SQLite", [==[SELECT [order], `group` FROM t WHERE a = ?1 AND b = @b AND c = $c AND d = :d AND e = ?;
INSERT INTO t VALUES (X'AB', 'x') /* not /* nested */;]==], {
    { 1, "[order]", "property" }, { 1, "`group`", "property" }, { 1, "?1", "variable.special" },
    { 1, "@b", "variable.special" }, { 1, "$c", "variable.special" }, { 1, ":d", "variable.special" },
    { 1, "?", "variable.special", 2 }, { 2, "X'AB'", "string" }, { 2, "/* not /* nested */", "comment" },
})
"##,
    );
    all_ok(&said, &["SQL", "PostgreSQL", "SQLite"]);
}

/// Every language's reader takes a line of 10 000 characters of its own punctuation and of what
/// opens its strings and comments, a language a frame (each call of the page has its time): it
/// bounds its work on a line, and reads at least a character each token.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn every_reader_reads_a_line_of_ten_thousand_characters_of_its_hard_cases_in_a_call() {
    let script = r##"
local languages = require("@pencere/languages")
local list = languages.list()
local done = 0
local said = {}
local PIECES = { "[", "*", "_", "<", "'", '"', "/", "\\", "`", "{", "}", "$", "#", "-", ":", "=", "a", " ", "(", "!", "~", "@", "?", "&", "|", ".", "%", "\t", "é" }
local line = {}
local length = 0
local k = 0
while length < 10000 do
    k += 1
    local piece = PIECES[(k * 7) % #PIECES + 1]
    line[#line + 1] = piece
    length += 1
end
line = table.concat(line)
return {
    render = function()
        done += 1
        local description = list[done]
        if description then
            local language = languages.load(description)
            local tokens = language.syntax:tokens(line)
            local far = tokens[#tokens]
            said[#said + 1] = description.name .. " " .. tostring(far == nil or far.to <= 10000)
        end
        local column = pencere.div():child(pencere.text(done > #list and "done" or "reading"))
        for _, words in said do
            column = column:child(pencere.text(words))
        end
        return column, done <= #list
    end,
}
"##;
    let mut page = open_script(script, ModuleLimits::default());
    let mut now = 0.0;
    let mut shown = Vec::new();
    for _ in 0..64 {
        page.module.render(SIZE, now, false);
        let update = page.until(Duration::from_secs(20), |u| {
            u.frame.is_some() || u.closed.is_some()
        });
        assert_eq!(update.closed, None, "{:?}", update.closed);
        shown = words(&update.frame.as_ref().expect("a page").root);
        if shown.first().map(String::as_str) == Some("done") {
            break;
        }
        now += 0.016;
    }
    assert_eq!(shown.first().map(String::as_str), Some("done"), "{shown:?}");
    assert!(shown.len() > 32, "{shown:?}");
    for words in &shown[1..] {
        assert!(words.ends_with(" true"), "{words}");
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_file_is_told_by_its_whole_name_its_extension_and_its_first_line_and_none_is_plain_text() {
    let said = said(
        r##"
local function named(path, first_line)
    local found = languages.detect(path, first_line)
    say((path or "nil") .. " | " .. (first_line or "") .. " -> " .. (found and found.name or "nil"))
end
named("src/main.rs")
named("SRC/MAIN.RS")
named("lib/app.TSX")
named("/home/me/project/Makefile")
named("makefile")
named("GNUmakefile")
named("Dockerfile")
named("Dockerfile.dev")
named("containerfile-prod")
named(".bashrc")
named(".env")
named(".env.local")
named("Cargo.lock")
named("notes.txt")
named("README")
named("C:\\work\\query.sql")
named("schema.pgsql")
named("archive.tar.gz")
named("script", "#!/usr/bin/env python3.12")
named("script", "#!/usr/bin/env -S deno run --allow-read")
named("script", "#!/usr/bin/env FOO=1 node")
named("script", "#!/bin/bash -e")
named("tool", "#!/usr/local/bin/lua5.4")
named("run", "#!/usr/bin/make -f")
named("unknown", "#!/usr/bin/perl")
named("page", "<?xml version=\"1.0\"?>")
named("index", "<!DOCTYPE html>")
named("index", "<?php echo 1;")
named("change", "diff --git a/x b/x")
named(nil, "#!/bin/sh")
named(nil, "just words")
named(nil, nil)
named("main.py", "#!/bin/sh")
"##,
    );
    assert_eq!(
        said,
        [
            "src/main.rs |  -> Rust",
            "SRC/MAIN.RS |  -> Rust",
            "lib/app.TSX |  -> TSX",
            "/home/me/project/Makefile |  -> Makefile",
            "makefile |  -> Makefile",
            "GNUmakefile |  -> Makefile",
            "Dockerfile |  -> Dockerfile",
            "Dockerfile.dev |  -> Dockerfile",
            "containerfile-prod |  -> Dockerfile",
            ".bashrc |  -> Shell",
            ".env |  -> INI",
            ".env.local |  -> INI",
            "Cargo.lock |  -> TOML",
            "notes.txt |  -> nil",
            "README |  -> nil",
            "C:\\work\\query.sql |  -> SQL",
            "schema.pgsql |  -> PostgreSQL",
            "archive.tar.gz |  -> nil",
            "script | #!/usr/bin/env python3.12 -> Python",
            "script | #!/usr/bin/env -S deno run --allow-read -> JavaScript",
            "script | #!/usr/bin/env FOO=1 node -> JavaScript",
            "script | #!/bin/bash -e -> Shell",
            "tool | #!/usr/local/bin/lua5.4 -> Lua",
            "run | #!/usr/bin/make -f -> Makefile",
            "unknown | #!/usr/bin/perl -> nil",
            "page | <?xml version=\"1.0\"?> -> XML",
            "index | <!DOCTYPE html> -> HTML",
            "index | <?php echo 1; -> PHP",
            "change | diff --git a/x b/x -> Diff",
            "nil | #!/bin/sh -> Shell",
            "nil | just words -> nil",
            "nil |  -> nil",
            "main.py | #!/bin/sh -> Python",
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_language_is_found_by_its_name_or_any_alias_in_any_case_and_the_list_is_every_one_by_name() {
    let said = said(
        r##"
for _, name in { "rust", "RS", "Rust", "golang", "C#", "csharp", "c++", "CPP", "js", "Node", "tsx", "py",
    "python3", "sh", "Bash", "zsh", "luau", "htm", "svg", "md", "yml", "jsonc", "toml", "dotenv",
    "sass", "less", "docker", "mk", "patch", "sql", "postgres", "PSQL", "pgsql", "sqlite", "sqlite3",
    "plain text", "", "nothing" } do
    local found = languages.find(name)
    say(name .. " -> " .. (found and found.name or "nil"))
end
local list = languages.list()
local names = {}
for k, description in list do
    names[k] = description.name
    if k > 1 and string.lower(list[k - 1].name) >= string.lower(description.name) then
        say("not sorted at " .. description.name)
    end
end
say(#list .. ": " .. table.concat(names, " "))
-- A new list each time, and the descriptions the module's own, which a page cannot change.
list[1] = nil
say(tostring(languages.list()[1] ~= nil))
say(tostring(pcall(function() languages.find("Rust").name = "Iron" end)))
say(tostring(pcall(function() table.insert(languages.find("Rust").aliases, "iron") end)))
say(tostring(languages.find("Rust").family) .. " " .. tostring(languages.find("SQL").family))
"##,
    );
    let (found, rest) = said.split_at(38);
    assert_eq!(
        found,
        [
            "rust -> Rust",
            "RS -> Rust",
            "Rust -> Rust",
            "golang -> Go",
            "C# -> C#",
            "csharp -> C#",
            "c++ -> C++",
            "CPP -> C++",
            "js -> JavaScript",
            "Node -> JavaScript",
            "tsx -> TSX",
            "py -> Python",
            "python3 -> Python",
            "sh -> Shell",
            "Bash -> Shell",
            "zsh -> Shell",
            "luau -> Lua",
            "htm -> HTML",
            "svg -> XML",
            "md -> Markdown",
            "yml -> YAML",
            "jsonc -> JSON",
            "toml -> TOML",
            "dotenv -> INI",
            "sass -> SCSS",
            "less -> Less",
            "docker -> Dockerfile",
            "mk -> Makefile",
            "patch -> Diff",
            "sql -> SQL",
            "postgres -> PostgreSQL",
            "PSQL -> PostgreSQL",
            "pgsql -> PostgreSQL",
            "sqlite -> SQLite",
            "sqlite3 -> SQLite",
            "plain text -> nil",
            " -> nil",
            "nothing -> nil",
        ]
    );
    assert_eq!(
        rest,
        [
            "33: C C# C++ CSS Diff Dockerfile Go HTML INI Java JavaScript JSON JSX Kotlin Less Lua \
             Makefile Markdown PHP PostgreSQL Python Ruby Rust SCSS Shell SQL SQLite Swift TOML TSX \
             TypeScript XML YAML",
            "true",
            "false",
            "false",
            "clike sql",
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn what_each_language_tells_an_editor_is_its_comments_indent_folds_brackets_and_tabs() {
    let said = said(
        r##"
local function shown(value)
    if type(value) ~= "table" then
        return tostring(value)
    end
    if value.open then
        return value.open .. " " .. value.close
    end
    if value[1] then
        return value[1] .. " " .. value[2]
    end
    return (value.line or "-") .. " " .. (value.block and shown(value.block) or "-")
end
for _, description in languages.list() do
    local config = languages.load(description).config
    local keys = {}
    for key in config do
        keys[#keys + 1] = key
    end
    table.sort(keys)
    say(description.name .. ": " .. table.concat(keys, ",") .. " | " .. shown(config.comment)
        .. " | " .. tostring(config.pairs) .. " | " .. tostring(config.folds)
        .. " | " .. tostring(config.hard_tabs) .. " " .. tostring(config.tab_width))
end
"##,
    );
    assert_eq!(
        said,
        [
            "C: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\" | indent | nil nil",
            "C#: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\" | indent | nil nil",
            "C++: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\" | indent | nil nil",
            "CSS: comment,folds,indent,pairs | - /* */ | ()[]{}''\"\" | indent | nil nil",
            "Diff:  | nil | nil | nil | nil nil",
            "Dockerfile: comment,folds,pairs | # | ()[]{}''\"\" | indent | nil nil",
            "Go: comment,folds,hard_tabs,indent,pairs | // /* */ | ()[]{}''\"\"`` | indent | true nil",
            "HTML: comment,folds,indent,pairs | - <!-- --> | ()[]{}\"\"'' | indent | nil nil",
            "INI: comment,folds,pairs | # | ()[]{}\"\" | indent | nil nil",
            "Java: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\" | indent | nil nil",
            "JavaScript: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\"`` | indent | nil nil",
            "JSON: comment,folds,indent,pairs | // /* */ | ()[]{}\"\" | indent | nil nil",
            "JSX: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\"`` | indent | nil nil",
            "Kotlin: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\" | indent | nil nil",
            "Less: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\" | indent | nil nil",
            "Lua: comment,folds,indent,pairs | -- --[[ ]] | ()[]{}''\"\" | indent | nil nil",
            "Makefile: comment,folds,hard_tabs,pairs | # | ()[]{}''\"\" | indent | true nil",
            "Markdown: comment,folds,pairs | - <!-- --> | ()[]{}`` | indent | nil nil",
            "PHP: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\" | indent | nil nil",
            "PostgreSQL: comment,folds,indent,pairs | -- /* */ | ()[]''\"\" | indent | nil nil",
            "Python: comment,folds,hard_tabs,indent,pairs,tab_width | # | ()[]{}''\"\" | indent | false 4",
            "Ruby: comment,folds,indent,pairs | # =begin =end | ()[]{}''\"\" | indent | nil nil",
            "Rust: comment,folds,hard_tabs,indent,pairs,tab_width | // /* */ | ()[]{}\"\" | indent | false 4",
            "SCSS: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\" | indent | nil nil",
            "Shell: comment,folds,indent,pairs | # | ()[]{}''\"\"`` | indent | nil nil",
            "SQL: comment,folds,indent,pairs | -- /* */ | ()[]''\"\"`` | indent | nil nil",
            "SQLite: comment,folds,indent,pairs | -- /* */ | ()[]''\"\"`` | indent | nil nil",
            "Swift: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\" | indent | nil nil",
            "TOML: comment,folds,pairs | # | ()[]{}''\"\" | indent | nil nil",
            "TSX: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\"`` | indent | nil nil",
            "TypeScript: comment,folds,indent,pairs | // /* */ | ()[]{}''\"\"`` | indent | nil nil",
            "XML: comment,folds,indent,pairs | - <!-- --> | ()[]{}\"\"'' | indent | nil nil",
            "YAML: comment,folds,hard_tabs,indent,pairs,tab_width | # | ()[]{}''\"\" | indent | false 2",
        ]
    );
}

/// An editor takes what every language tells it: the page that puts one editor of each language's
/// `config` on it is drawn, its rules of indent compiled as the editor compiles them (one that is
/// no pattern fails the page that says it).
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn an_editor_takes_what_every_language_tells_it() {
    let update = first(
        r#"
        local languages = require("@pencere/languages")
        return {
            render = function()
                local column = pencere.div():size_full()
                for _, description in languages.list() do
                    column = column:child(pencere.editor(description.name, languages.load(description).config):h(10))
                end
                return column:child(pencere.text("drawn"))
            end,
        }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    assert_eq!(
        words(&update.frame.as_ref().expect("a page").root),
        ["drawn"]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn requiring_the_module_builds_no_language_and_a_load_builds_the_one_asked_for_once() {
    let said = said(
        r##"
local built = {}
for _, description in languages.list() do
    if languages.loaded(description) then
        built[#built + 1] = description.name
    end
end
say("built at first: " .. #built)
local rust = languages.load("rust")
say(tostring(rawequal(rust, languages.load("Rust"))) .. " " .. tostring(rawequal(rust, languages.loaded("RS"))))
say(tostring(rawequal(rust.description, languages.find("Rust"))) .. " " .. rust.name)
say("Go " .. tostring(languages.loaded("Go") ~= nil) .. ", Python " .. tostring(languages.loaded("Python") ~= nil))
say(tostring(languages.load("nothing")) .. " " .. tostring(languages.loaded("nothing")))
-- A page of HTML builds JavaScript when it first meets a script, and not before.
local html = languages.load("HTML")
html.syntax:tokens("<p>no script</p>")
say("JavaScript " .. tostring(languages.loaded("JavaScript") ~= nil))
html.syntax:tokens("<script>let a</script>")
say("JavaScript " .. tostring(languages.loaded("JavaScript") ~= nil))
-- The language and its config are the module's: a page changes a copy.
say(tostring(pcall(function() rust.config.hint = "x" end)))
local mine = table.clone(rust.config)
mine.hint = "x"
say(mine.hint .. " " .. tostring(rust.config.hint))
"##,
    );
    assert_eq!(
        said,
        [
            "built at first: 0",
            "true true",
            "true Rust",
            "Go false, Python false",
            "nil nil",
            "JavaScript false",
            "JavaScript true",
            "false",
            "x nil",
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_name_that_is_no_name_is_a_mistake_said_at_the_callers_line() {
    for (call, says) in [
        (
            "languages.find(5)",
            "find takes the name of a language (a string), not a number",
        ),
        (
            "languages.load({})",
            "a language is said by its name or by its description (languages.find), not another table",
        ),
        (
            "languages.load(true)",
            "a language is said by its name (a string) or its description, not a boolean",
        ),
        (
            "languages.detect(5)",
            "detect takes the path of a file (a string, or nil), not a number",
        ),
        (
            "languages.detect('a.rs', 5)",
            "detect takes the first line of the file (a string, or nil), not a number",
        ),
        (
            "languages.look(5)",
            "look takes the name of a token (a string), not a number",
        ),
    ] {
        let update = first(
            &format!("local languages = require('@pencere/languages')\n\n{call}\nreturn {{}}"),
            ModuleLimits::default(),
        );
        let why = failed(&update);
        assert!(why.contains(":3:"), "{call}: {why}");
        assert!(why.contains(says), "{call}: {why}");
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_looks_are_the_themes_roles_and_a_name_falls_back_as_an_editors_does() {
    let said = said(
        r##"
local function shown(look)
    if look == nil then
        return "nil"
    end
    local keys = {}
    for key, value in look do
        keys[#keys + 1] = key .. "=" .. (type(value) == "table" and (value.toward and (tostring(value[1]) .. ">" .. tostring(value.toward)) or "{}") or tostring(value))
    end
    table.sort(keys)
    return table.concat(keys, " ")
end
for _, name in { "keyword", "string.special", "string.special.deep", "list atom", "heading strong", "comment", "unknown" } do
    say(name .. ": " .. shown(languages.look(name)))
end
local Syntax = require("@pencere/syntax")
local ok = pcall(Syntax.define, { name = "x", start = function() return {} end, token = function(s) s:next() end, styles = languages.styles })
say("define takes the styles: " .. tostring(ok))
say(tostring(pcall(function() languages.styles.keyword = {} end)))
say(tostring(pcall(function() languages.look("invalid").underline.wavy = "accent" end)))
"##,
    );
    assert_eq!(
        said,
        [
            "keyword: color=accent",
            "string.special: color=error>warning",
            "string.special.deep: color=error>warning",
            "list atom: color=warning",
            "heading strong: color=accent weight=700",
            "comment: color=dim>quiet italic=true",
            "unknown: nil",
            "define takes the styles: true",
            "false",
            "false",
        ]
    );
}

/// What requiring the module costs a page, against what loading all its languages does: a
/// measure, said with `--nocapture`, for a person to read (the time to start a page is most of
/// either). What the test holds is what the cost follows from: requiring runs the module's own
/// script, the descriptions, and none of the families', which are the most of its text.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn requiring_the_module_runs_its_descriptions_alone_which_are_the_least_of_it() {
    let page = |body: &str| {
        format!("{body}\nreturn {{ render = function() return pencere.text(\"ready\") end }}")
    };
    let time = |script: &str| {
        let mut best = Duration::MAX;
        for _ in 0..5 {
            let began = Instant::now();
            let update = first(script, ModuleLimits::default());
            assert_eq!(update.closed, None, "{:?}", update.closed);
            best = best.min(began.elapsed());
        }
        best
    };
    // The interpreter is compiled once, before any page is timed.
    let nothing = time(&page(""));
    let required = time(&page("local languages = require('@pencere/languages')"));
    let loaded = time(&page(
        "local languages = require('@pencere/languages')\n\
         for _, d in languages.list() do languages.load(d) end",
    ));
    println!(
        "a page: {nothing:?}; one that requires @pencere/languages: {required:?}; one that loads every language: {loaded:?}"
    );
    let size = |path: &str| library::source(path).map_or(0, str::len);
    let module = size("@pencere/languages/init.luau");
    let families: usize = library::MODULES
        .iter()
        .filter(|(path, _)| {
            path.starts_with("@pencere/languages/") && !path.ends_with("/init.luau")
        })
        .map(|(_, source)| source.len())
        .sum();
    assert!(module > 0 && families > 3 * module, "{module} {families}");
}

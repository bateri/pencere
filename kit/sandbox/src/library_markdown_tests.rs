//! `@pencere/markdown`: Markdown read into a tree of blocks and inlines, kept for a text that
//! changes, and drawn as elements. Each test is a page that reads what it is given and says what it
//! read as words: a tree is said in a short form of its own (`DUMP`), compared with the trees the
//! specifications' examples come to. The examples are CommonMark's (0.31.2) and GitHub's (GFM 0.29),
//! by their numbers there; their HTML was read into the same form. Heavy: the interpreter is
//! compiled and runs the pages (`make check-heavy`).

use std::fmt::Write as _;
use std::time::Duration;

use pencere_view::Element;
use pencere_view::wire::Kind;

use crate::guests_tests::SIZE;
use crate::luau_tests::{failed, first, open_script, words};
use crate::{ModuleLimits, Update};

/// What a page that reads Markdown says of a tree, as one line: a block as its kind and what is in
/// it (`h2[...]`, `list(3,loose)[item[...]]`, `code(lua)"..."`, `table(l,c,-)[head[cell[...]],row[...]]`),
/// an inline as its kind (`"text"`, `em[...]`, `link("href","title")[...]`, `img("src","alt")`, `soft`).
const DUMP: &str = r#"
local function q(s)
    s = string.gsub(s, "\\", "\\\\")
    s = string.gsub(s, '"', '\\"')
    s = string.gsub(s, "\n", "\\n")
    s = string.gsub(s, "\t", "\\t")
    return '"' .. s .. '"'
end
local dump_inlines, dump_blocks
function dump_inlines(nodes)
    local out = {}
    for _, n in ipairs(nodes or {}) do
        local t = n.type
        local s
        if t == "text" then s = q(n.text)
        elseif t == "softbreak" then s = "soft"
        elseif t == "hardbreak" then s = "hard"
        elseif t == "emphasis" then s = "em[" .. dump_inlines(n.children) .. "]"
        elseif t == "strong" then s = "strong[" .. dump_inlines(n.children) .. "]"
        elseif t == "strike" then s = "del[" .. dump_inlines(n.children) .. "]"
        elseif t == "code" then s = "code" .. q(n.text)
        elseif t == "html" then s = "html" .. q(n.text)
        elseif t == "link" then
            s = "link(" .. q(n.href) .. ((n.title and n.title ~= "") and ("," .. q(n.title)) or "") .. ")[" .. dump_inlines(n.children) .. "]"
        elseif t == "image" then
            s = "img(" .. q(n.src) .. "," .. q(n.alt) .. ((n.title and n.title ~= "") and ("," .. q(n.title)) or "") .. ")"
        else s = "?" .. tostring(t) end
        out[#out + 1] = s
    end
    return table.concat(out, ",")
end
local ALIGN = { left = "l", center = "c", right = "r" }
local function dump_block(b)
    local t = b.type
    if t == "paragraph" then return "para[" .. dump_inlines(b.children) .. "]"
    elseif t == "heading" then return "h" .. b.level .. "[" .. dump_inlines(b.children) .. "]"
    elseif t == "quote" then return "quote[" .. dump_blocks(b.children) .. "]"
    elseif t == "list" then
        return "list(" .. (b.ordered and tostring(b.start) or "-") .. "," .. (b.tight and "tight" or "loose") .. ")[" .. dump_blocks(b.children) .. "]"
    elseif t == "item" then
        local mark = b.checked == nil and "" or (b.checked and "(x)" or "( )")
        return "item" .. mark .. "[" .. dump_blocks(b.children) .. "]"
    elseif t == "code" then return "code" .. (b.language and ("(" .. b.language .. ")") or "") .. q(b.text)
    elseif t == "html" then return "html" .. q(b.text)
    elseif t == "rule" then return "rule"
    elseif t == "table" then
        local aligns = {}
        for i, a in ipairs(b.align) do aligns[i] = a and ALIGN[a] or "-" end
        local parts = {}
        local head = {}
        for i, cell in ipairs(b.head) do head[i] = "cell[" .. dump_inlines(cell) .. "]" end
        parts[1] = "head[" .. table.concat(head, ",") .. "]"
        for _, row in ipairs(b.rows) do
            local cells = {}
            for i, cell in ipairs(row) do cells[i] = "cell[" .. dump_inlines(cell) .. "]" end
            parts[#parts + 1] = "row[" .. table.concat(cells, ",") .. "]"
        end
        return "table(" .. table.concat(aligns, ",") .. ")[" .. table.concat(parts, ",") .. "]"
    end
    return "?" .. tostring(t)
end
function dump_blocks(nodes)
    local out = {}
    for _, b in ipairs(nodes or {}) do out[#out + 1] = dump_block(b) end
    return table.concat(out, ",")
end
local function dump(doc) return dump_blocks(doc.children) end
"#;

/// A string of Lua that is `text`: what is not printable ASCII written as `\ddd`.
fn lua(text: &str) -> String {
    let mut out = String::from("\"");
    for byte in text.bytes() {
        if (32..127).contains(&byte) && byte != b'"' && byte != b'\\' {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "\\{byte:03}");
        }
    }
    out.push('"');
    out
}

/// The words of the page a script makes, or why it failed.
fn said(script: &str) -> Vec<String> {
    let update = first(script, ModuleLimits::default());
    assert_eq!(update.closed, None, "{}", failed(&update));
    words(&update.frame.expect("a page").root)
}

/// The cases `(name, markdown, tree)` a page reads otherwise than the tree says, each with what it
/// read.
fn misread(cases: &[(u32, &str, &str)]) -> Vec<String> {
    let mut script =
        format!("{DUMP}\nlocal markdown = require('@pencere/markdown')\nlocal cases = {{\n");
    for (number, markdown, _) in cases {
        let _ = writeln!(script, "{{ {number}, {} }},", lua(markdown));
    }
    script.push_str(
        r#"}
        local lines = {}
        for _, case in ipairs(cases) do
            local ok, got = pcall(function() return dump(markdown.parse(case[2])) end)
            lines[#lines + 1] = case[1] .. "\t" .. tostring(got)
        end
        return { render = function()
            local root = pencere.div()
            for _, line in ipairs(lines) do root:child(pencere.text(line)) end
            return root
        end }
        "#,
    );
    let lines = said(&script);
    assert_eq!(lines.len(), cases.len(), "{lines:?}");
    cases
        .iter()
        .zip(&lines)
        .filter_map(|((number, markdown, want), line)| {
            let got = line.split_once('\t').map_or(line.as_str(), |(_, got)| got);
            (got != *want)
                .then(|| format!("example {number}: {markdown:?}\n   got: {got}\n  want: {want}"))
        })
        .collect()
}

/// Examples of the blocks of the CommonMark specification (0.31.2), by their numbers there: the Markdown, and the tree it is read to.
const COMMONMARK_BLOCKS: &[(u32, &str, &str)] = &[
    (1, "\tfoo\tbaz\t\tbim\n", r#"code"foo\tbaz\t\tbim""#),
    (6, ">\t\tfoo\n", r#"quote[code"  foo"]"#),
    (
        12,
        "\\!\\\"\\#\\$\\%\\&\\'\\(\\)\\*\\+\\,\\-\\.\\/\\:\\;\\<\\=\\>\\?\\@\\[\\\\\\]\\^\\_\\`\\{\\|\\}\\~\n",
        "para[\"!\\\"#$%&'()*+,-./:;<=>?@[\\\\]^_`{|}~\"]",
    ),
    (16, "foo\\\nbar\n", r#"para["foo",hard,"bar"]"#),
    (
        20,
        "<https://example.com?find=\\*>\n",
        r#"para[link("https://example.com?find=%5C*")["https://example.com?find=\\*"]]"#,
    ),
    (
        25,
        "&nbsp; &amp; &copy; &AElig; &Dcaron;\n&frac34; &HilbertSpace; &DifferentialD;\n&ClockwiseContourIntegral; &ngE;\n",
        r#"para["  & © Æ Ď",soft,"¾ ℋ ⅆ",soft,"∲ ≧̸"]"#,
    ),
    (29, "&copy\n", r#"para["&copy"]"#),
    (
        33,
        "[foo]\n\n[foo]: /f&ouml;&ouml; \"f&ouml;&ouml;\"\n",
        r#"para[link("/f%C3%B6%C3%B6","föö")["foo"]]"#,
    ),
    (
        37,
        "&#42;foo&#42;\n*foo*\n",
        r#"para["*foo*",soft,em["foo"]]"#,
    ),
    (
        42,
        "- `one\n- two`\n",
        r#"list(-,tight)[item[para["`one"]],item[para["two`"]]]"#,
    ),
    (43, "***\n---\n___\n", r#"rule,rule,rule"#),
    (47, " ***\n  ***\n   ***\n", r#"rule,rule,rule"#),
    (52, " **  * ** * ** * **\n", r#"rule"#),
    (
        57,
        "- foo\n***\n- bar\n",
        r#"list(-,tight)[item[para["foo"]]],rule,list(-,tight)[item[para["bar"]]]"#,
    ),
    (
        62,
        "# foo\n## foo\n### foo\n#### foo\n##### foo\n###### foo\n",
        r#"h1["foo"],h2["foo"],h3["foo"],h4["foo"],h5["foo"],h6["foo"]"#,
    ),
    (
        66,
        "# foo *bar* \\*baz\\*\n",
        r#"h1["foo ",em["bar"]," *baz*"]"#,
    ),
    (
        71,
        "## foo ##\n  ###   bar    ###\n",
        r#"h2["foo"],h3["bar"]"#,
    ),
    (75, "# foo#\n", r#"h1["foo#"]"#),
    (
        80,
        "Foo *bar*\n=========\n\nFoo *bar*\n---------\n",
        r#"h1["Foo ",em["bar"]],h2["Foo ",em["bar"]]"#,
    ),
    (
        84,
        "   Foo\n---\n\n  Foo\n-----\n\n  Foo\n  ===\n",
        r#"h2["Foo"],h2["Foo"],h1["Foo"]"#,
    ),
    (89, "Foo  \n-----\n", r#"h2["Foo"]"#),
    (
        93,
        "> foo\nbar\n===\n",
        r#"quote[para["foo",soft,"bar",soft,"==="]]"#,
    ),
    (98, "---\n---\n", r#"rule,rule"#),
    (102, "\\> foo\n------\n", r#"h2["> foo"]"#),
    (
        107,
        "    a simple\n      indented code block\n",
        r#"code"a simple\n  indented code block""#,
    ),
    (
        111,
        "    chunk1\n\n    chunk2\n  \n \n \n    chunk3\n",
        r#"code"chunk1\n\nchunk2\n\n\n\nchunk3""#,
    ),
    (
        115,
        "# Heading\n    foo\nHeading\n------\n    foo\n----\n",
        r#"h1["Heading"],code"foo",h2["Heading"],code"foo",rule"#,
    ),
    (119, "```\n<\n >\n```\n", r#"code"<\n >""#),
    (123, "~~~\naaa\n```\n~~~\n", r#"code"aaa\n```""#),
    (
        128,
        "> ```\n> aaa\n\nbbb\n",
        r#"quote[code"aaa"],para["bbb"]"#,
    ),
    (
        133,
        "   ```\n   aaa\n    aaa\n  aaa\n   ```\n",
        r#"code"aaa\n aaa\naaa""#,
    ),
    (138, "``` ```\naaa\n", r#"para[code" ",soft,"aaa"]"#),
    (
        143,
        "~~~~    ruby startline=3 $%@#$\ndef foo(x)\n  return 3\nend\n~~~~~~~\n",
        r#"code(ruby)"def foo(x)\n  return 3\nend""#,
    ),
    (
        148,
        "<table><tr><td>\n<pre>\n**Hello**,\n\n_world_.\n</pre>\n</td></tr></table>\n",
        r#"html"<table><tr><td>\n<pre>\n**Hello**,",para[em["world"],".",soft,html"</pre>"],html"</td></tr></table>""#,
    ),
    (
        152,
        "<DIV CLASS=\"foo\">\n\n*Markdown*\n\n</DIV>\n",
        r#"html"<DIV CLASS=\"foo\">",para[em["Markdown"]],html"</DIV>""#,
    ),
    (
        156,
        "<div id=\"foo\"\n*hi*\n",
        r#"html"<div id=\"foo\"\n*hi*""#,
    ),
    (
        161,
        "<div></div>\n``` c\nint x = 33;\n```\n",
        r#"html"<div></div>\n``` c\nint x = 33;\n```""#,
    ),
    (165, "</ins>\n*bar*\n", r#"html"</ins>\n*bar*""#),
    (
        170,
        "<script type=\"text/javascript\">\n// JavaScript example\n\ndocument.getElementById(\"demo\").innerHTML = \"Hello JavaScript!\";\n</script>\nokay\n",
        r#"html"<script type=\"text/javascript\">\n// JavaScript example\n\ndocument.getElementById(\"demo\").innerHTML = \"Hello JavaScript!\";\n</script>",para["okay"]"#,
    ),
    (
        174,
        "> <div>\n> foo\n\nbar\n",
        r#"quote[html"<div>\nfoo"],para["bar"]"#,
    ),
    (
        178,
        "<script>\nfoo\n</script>1. *bar*\n",
        r#"html"<script>\nfoo\n</script>1. *bar*""#,
    ),
    (
        183,
        "  <!-- foo -->\n\n    <!-- foo -->\n",
        r#"html"  <!-- foo -->",code"<!-- foo -->""#,
    ),
    (
        187,
        "Foo\n<a href=\"bar\">\nbaz\n",
        r#"para["Foo",soft,html"<a href=\"bar\">",soft,"baz"]"#,
    ),
    (
        192,
        "[foo]: /url \"title\"\n\n[foo]\n",
        r#"para[link("/url","title")["foo"]]"#,
    ),
    (
        196,
        "[foo]: /url '\ntitle\nline1\nline2\n'\n\n[foo]\n",
        r#"para[link("/url","\ntitle\nline1\nline2\n")["foo"]]"#,
    ),
    (
        201,
        "[foo]: <bar>(baz)\n\n[foo]\n",
        r#"para["[foo]: ",html"<bar>","(baz)"],para["[foo]"]"#,
    ),
    (
        205,
        "[FOO]: /url\n\n[Foo]\n",
        r#"para[link("/url")["Foo"]]"#,
    ),
    (
        210,
        "[foo]: /url\n\"title\" ok\n",
        r#"para["\"title\" ok"]"#,
    ),
    (
        214,
        "# [Foo]\n[foo]: /url\n> bar\n",
        r#"h1[link("/url")["Foo"]],quote[para["bar"]]"#,
    ),
    (219, "aaa\n\nbbb\n", r#"para["aaa"],para["bbb"]"#),
    (
        223,
        "aaa\n             bbb\n                                       ccc\n",
        r#"para["aaa",soft,"bbb",soft,"ccc"]"#,
    ),
    (
        227,
        "  \n\naaa\n  \n\n# aaa\n\n  \n",
        r#"para["aaa"],h1["aaa"]"#,
    ),
    (
        228,
        "> # Foo\n> bar\n> baz\n",
        r#"quote[h1["Foo"],para["bar",soft,"baz"]]"#,
    ),
    (
        232,
        "> # Foo\n> bar\nbaz\n",
        r#"quote[h1["Foo"],para["bar",soft,"baz"]]"#,
    ),
    (236, ">     foo\n    bar\n", r#"quote[code"foo"],code"bar""#),
    (240, ">\n>  \n> \n", r#"quote[]"#),
    (
        244,
        "> foo\n>\n> bar\n",
        r#"quote[para["foo"],para["bar"]]"#,
    ),
    (248, "> bar\n\nbaz\n", r#"quote[para["bar"]],para["baz"]"#),
    (
        253,
        "A paragraph\nwith two lines.\n\n    indented code\n\n> A block quote.\n",
        r#"para["A paragraph",soft,"with two lines."],code"indented code",quote[para["A block quote."]]"#,
    ),
    (
        257,
        " -    one\n\n     two\n",
        r#"list(-,tight)[item[para["one"]]],code" two""#,
    ),
    (261, "-one\n\n2.two\n", r#"para["-one"],para["2.two"]"#),
    (266, "1234567890. not ok\n", r#"para["1234567890. not ok"]"#),
    (
        270,
        "- foo\n\n      bar\n",
        r#"list(-,loose)[item[para["foo"],code"bar"]]"#,
    ),
    (
        274,
        "1.      indented code\n\n   paragraph\n\n       more code\n",
        r#"list(1,loose)[item[code" indented code",para["paragraph"],code"more code"]]"#,
    ),
    (279, "-   \n  foo\n", r#"list(-,tight)[item[para["foo"]]]"#),
    (
        283,
        "1. foo\n2.\n3. bar\n",
        r#"list(1,tight)[item[para["foo"]],item[],item[para["bar"]]]"#,
    ),
    (
        287,
        "  1.  A paragraph\n      with two lines.\n\n          indented code\n\n      > A block quote.\n",
        r#"list(1,loose)[item[para["A paragraph",soft,"with two lines."],code"indented code",quote[para["A block quote."]]]]"#,
    ),
    (
        292,
        "> 1. > Blockquote\ncontinued here.\n",
        r#"quote[list(1,tight)[item[quote[para["Blockquote",soft,"continued here."]]]]]"#,
    ),
    (
        296,
        "10) foo\n    - bar\n",
        r#"list(10,tight)[item[para["foo"],list(-,tight)[item[para["bar"]]]]]"#,
    ),
    (
        301,
        "- foo\n- bar\n+ baz\n",
        r#"list(-,tight)[item[para["foo"]],item[para["bar"]]],list(-,tight)[item[para["baz"]]]"#,
    ),
    (
        305,
        "The number of windows in my house is\n1.  The number of doors is 6.\n",
        r#"para["The number of windows in my house is"],list(1,tight)[item[para["The number of doors is 6."]]]"#,
    ),
    (
        309,
        "-   foo\n\n    notcode\n\n-   foo\n\n<!-- -->\n\n    code\n",
        r#"list(-,loose)[item[para["foo"],para["notcode"]],item[para["foo"]]],html"<!-- -->",code"code""#,
    ),
    (
        314,
        "- a\n- b\n\n- c\n",
        r#"list(-,loose)[item[para["a"]],item[para["b"]],item[para["c"]]]"#,
    ),
    (
        318,
        "- a\n- ```\n  b\n\n\n  ```\n- c\n",
        r#"list(-,tight)[item[para["a"]],item[code"b\n\n"],item[para["c"]]]"#,
    ),
    (322, "- a\n", r#"list(-,tight)[item[para["a"]]]"#),
];

/// Examples of the inlines of the CommonMark specification (0.31.2), by their numbers there.
const COMMONMARK_INLINES: &[(u32, &str, &str)] = &[
    (327, "`hi`lo`\n", r#"para[code"hi","lo`"]"#),
    (328, "`foo`\n", r#"para[code"foo"]"#),
    (332, "` a`\n", r#"para[code" a"]"#),
    (336, "``\nfoo \n``\n", r#"para[code"foo "]"#),
    (341, "*foo`*`\n", r#"para["*foo",code"*"]"#),
    (
        345,
        "`<https://foo.bar.`baz>`\n",
        r#"para[code"<https://foo.bar.","baz>`"]"#,
    ),
    (350, "*foo bar*\n", r#"para[em["foo bar"]]"#),
    (
        354,
        "*$*alpha.\n\n*£*bravo.\n\n*€*charlie.\n",
        r#"para["*$*alpha."],para["*£*bravo."],para["*€*charlie."]"#,
    ),
    (359, "a_\"foo\"_\n", r#"para["a_\"foo\"_"]"#),
    (363, "aa_\"bb\"_cc\n", r#"para["aa_\"bb\"_cc"]"#),
    (368, "*(*foo)\n", r#"para["*(*foo)"]"#),
    (372, "_(_foo)\n", r#"para["_(_foo)"]"#),
    (377, "_(bar)_.\n", r#"para[em["(bar)"],"."]"#),
    (381, "foo**bar**\n", r#"para["foo",strong["bar"]]"#),
    (386, "foo__bar__\n", r#"para["foo__bar__"]"#),
    (390, "foo-__(bar)__\n", r#"para["foo-",strong["(bar)"]]"#),
    (
        395,
        "**foo \"*bar*\" foo**\n",
        r#"para[strong["foo \"",em["bar"],"\" foo"]]"#,
    ),
    (400, "__foo__bar\n", r#"para["__foo__bar"]"#),
    (
        404,
        "*foo [bar](/url)*\n",
        r#"para[em["foo ",link("/url")["bar"]]]"#,
    ),
    (409, "*foo *bar**\n", r#"para[em["foo ",em["bar"]]]"#),
    (413, "***foo** bar*\n", r#"para[em[strong["foo"]," bar"]]"#),
    (
        418,
        "*foo **bar *baz* bim** bop*\n",
        r#"para[em["foo ",strong["bar ",em["baz"]," bim"]," bop"]]"#,
    ),
    (
        422,
        "**foo [bar](/url)**\n",
        r#"para[strong["foo ",link("/url")["bar"]]]"#,
    ),
    (
        427,
        "**foo **bar****\n",
        r#"para[strong["foo ",strong["bar"]]]"#,
    ),
    (431, "**foo *bar***\n", r#"para[strong["foo ",em["bar"]]]"#),
    (436, "foo ***\n", r#"para["foo ***"]"#),
    (441, "foo **_**\n", r#"para["foo ",strong["_"]]"#),
    (445, "****foo*\n", r#"para["***",em["foo"]]"#),
    (450, "foo _*_\n", r#"para["foo ",em["*"]]"#),
    (454, "__foo_\n", r#"para["_",em["foo"]]"#),
    (459, "_foo____\n", r#"para[em["foo"],"___"]"#),
    (463, "_*foo*_\n", r#"para[em[em["foo"]]]"#),
    (468, "_____foo_____\n", r#"para[em[strong[strong["foo"]]]]"#),
    (472, "*foo *bar baz*\n", r#"para["*foo ",em["bar baz"]]"#),
    (
        477,
        "__<a href=\"__\">\n",
        r#"para["__",html"<a href=\"__\">"]"#,
    ),
    (
        482,
        "[link](/uri \"title\")\n",
        r#"para[link("/uri","title")["link"]]"#,
    ),
    (486, "[link](<>)\n", r#"para[link("")["link"]]"#),
    (
        491,
        "[link](<foo\nbar>)\n",
        r#"para["[link](",html"<foo\nbar>",")"]"#,
    ),
    (495, "[link](\\(foo\\))\n", r#"para[link("(foo)")["link"]]"#),
    (500, "[link](foo\\)\\:)\n", r#"para[link("foo):")["link"]]"#),
    (
        504,
        "[link](\"title\")\n",
        r#"para[link("%22title%22")["link"]]"#,
    ),
    (
        509,
        "[link](/url 'title \"and\" title')\n",
        r#"para[link("/url","title \"and\" title")["link"]]"#,
    ),
    (513, "[link] bar](/uri)\n", r#"para["[link] bar](/uri)"]"#),
    (
        518,
        "[foo [bar](/uri)](/uri)\n",
        r#"para["[foo ",link("/uri")["bar"],"](/uri)"]"#,
    ),
    (
        522,
        "[foo *bar](baz*)\n",
        r#"para[link("baz*")["foo *bar"]]"#,
    ),
    (
        527,
        "[foo][bar]\n\n[bar]: /url \"title\"\n",
        r#"para[link("/url","title")["foo"]]"#,
    ),
    (
        531,
        "[![moon](moon.jpg)][ref]\n\n[ref]: /uri\n",
        r#"para[link("/uri")[img("moon.jpg","moon")]]"#,
    ),
    (
        536,
        "[foo <bar attr=\"][ref]\">\n\n[ref]: /uri\n",
        r#"para["[foo ",html"<bar attr=\"][ref]\">"]"#,
    ),
    (540, "[ẞ]\n\n[SS]: /url\n", r#"para[link("/url")["ẞ"]]"#),
    (
        545,
        "[bar][foo\\!]\n\n[foo!]: /url\n",
        r#"para["[bar][foo!]"]"#,
    ),
    (
        549,
        "[foo][ref\\[]\n\n[ref\\[]: /uri\n",
        r#"para[link("/uri")["foo"]]"#,
    ),
    (
        554,
        "[*foo* bar][]\n\n[*foo* bar]: /url \"title\"\n",
        r#"para[link("/url","title")[em["foo"]," bar"]]"#,
    ),
    (
        558,
        "[*foo* bar]\n\n[*foo* bar]: /url \"title\"\n",
        r#"para[link("/url","title")[em["foo"]," bar"]]"#,
    ),
    (
        563,
        "\\[foo]\n\n[foo]: /url \"title\"\n",
        r#"para["[foo]"]"#,
    ),
    (567, "[foo]()\n\n[foo]: /url1\n", r#"para[link("")["foo"]]"#),
    (
        572,
        "![foo](/url \"title\")\n",
        r#"para[img("/url","foo","title")]"#,
    ),
    (
        576,
        "![foo *bar*][]\n\n[foo *bar*]: train.jpg \"train & tracks\"\n",
        r#"para[img("train.jpg","foo bar","train & tracks")]"#,
    ),
    (580, "![foo](<url>)\n", r#"para[img("url","foo")]"#),
    (
        585,
        "![*foo* bar][]\n\n[*foo* bar]: /url \"title\"\n",
        r#"para[img("/url","foo bar","title")]"#,
    ),
    (
        589,
        "![*foo* bar]\n\n[*foo* bar]: /url \"title\"\n",
        r#"para[img("/url","foo bar","title")]"#,
    ),
    (
        594,
        "<http://foo.bar.baz>\n",
        r#"para[link("http://foo.bar.baz")["http://foo.bar.baz"]]"#,
    ),
    (598, "<a+b+c:d>\n", r#"para[link("a+b+c:d")["a+b+c:d"]]"#),
    (
        602,
        "<https://foo.bar/baz bim>\n",
        r#"para["<https://foo.bar/baz bim>"]"#,
    ),
    (
        606,
        "<foo\\+@bar.example.com>\n",
        r#"para["<",link("mailto:foo+@bar.example.com")["foo+@bar.example.com"],">"]"#,
    ),
    (
        613,
        "<a><bab><c2c>\n",
        r#"para[html"<a>",html"<bab>",html"<c2c>"]"#,
    ),
    (618, "<33> <__>\n", r#"para["<33> <__>"]"#),
    (623, "</a></foo >\n", r#"para[html"</a>",html"</foo >"]"#),
    (
        628,
        "foo <!ELEMENT br EMPTY>\n",
        r#"para["foo ",html"<!ELEMENT br EMPTY>"]"#,
    ),
    (633, "foo  \nbaz\n", r#"para["foo",hard,"baz"]"#),
    (638, "*foo  \nbar*\n", r#"para[em["foo",hard,"bar"]]"#),
    (
        643,
        "<a href=\"foo\\\nbar\">\n",
        r#"para[html"<a href=\"foo\\\nbar\">"]"#,
    ),
    (648, "foo\nbaz\n", r#"para["foo",soft,"baz"]"#),
    (649, "foo \n baz\n", r#"para["foo",soft,"baz"]"#),
    (650, "hello $.;'there\n", r#"para["hello $.;'there"]"#),
    (651, "Foo χρῆν\n", r#"para["Foo χρῆν"]"#),
];

/// Examples of GitHub's extensions in its specification (GFM 0.29), by their numbers there.
const GFM: &[(u32, &str, &str)] = &[
    (
        198,
        "| foo | bar |\n| --- | --- |\n| baz | bim |\n",
        r#"table(-,-)[head[cell["foo"],cell["bar"]],row[cell["baz"],cell["bim"]]]"#,
    ),
    (
        199,
        "| abc | defghi |\n:-: | -----------:\nbar | baz\n",
        r#"table(c,r)[head[cell["abc"],cell["defghi"]],row[cell["bar"],cell["baz"]]]"#,
    ),
    (
        200,
        "| f\\|oo  |\n| ------ |\n| b `\\|` az |\n| b **\\|** im |\n",
        r#"table(-)[head[cell["f|oo"]],row[cell["b ",code"|"," az"]],row[cell["b ",strong["|"]," im"]]]"#,
    ),
    (
        201,
        "| abc | def |\n| --- | --- |\n| bar | baz |\n> bar\n",
        r#"table(-,-)[head[cell["abc"],cell["def"]],row[cell["bar"],cell["baz"]]],quote[para["bar"]]"#,
    ),
    (
        202,
        "| abc | def |\n| --- | --- |\n| bar | baz |\nbar\n\nbar\n",
        r#"table(-,-)[head[cell["abc"],cell["def"]],row[cell["bar"],cell["baz"]],row[cell["bar"],cell[]]],para["bar"]"#,
    ),
    (
        203,
        "| abc | def |\n| --- |\n| bar |\n",
        r#"para["| abc | def |",soft,"| --- |",soft,"| bar |"]"#,
    ),
    (
        204,
        "| abc | def |\n| --- | --- |\n| bar |\n| bar | baz | boo |\n",
        r#"table(-,-)[head[cell["abc"],cell["def"]],row[cell["bar"],cell[]],row[cell["bar"],cell["baz"]]]"#,
    ),
    (
        205,
        "| abc | def |\n| --- | --- |\n",
        r#"table(-,-)[head[cell["abc"],cell["def"]]]"#,
    ),
    (
        279,
        "- [ ] foo\n- [x] bar\n",
        r#"list(-,tight)[item( )[para["foo"]],item(x)[para["bar"]]]"#,
    ),
    (
        280,
        "- [x] foo\n  - [ ] bar\n  - [x] baz\n- [ ] bim\n",
        r#"list(-,tight)[item(x)[para["foo"],list(-,tight)[item( )[para["bar"]],item(x)[para["baz"]]]],item( )[para["bim"]]]"#,
    ),
    (
        491,
        "~~Hi~~ Hello, world!\n",
        r#"para[del["Hi"]," Hello, world!"]"#,
    ),
    (
        492,
        "This ~~has a\n\nnew paragraph~~.\n",
        r#"para["This ~~has a"],para["new paragraph~~."]"#,
    ),
    (
        621,
        "www.commonmark.org\n",
        r#"para[link("http://www.commonmark.org")["www.commonmark.org"]]"#,
    ),
    (
        622,
        "Visit www.commonmark.org/help for more information.\n",
        r#"para["Visit ",link("http://www.commonmark.org/help")["www.commonmark.org/help"]," for more information."]"#,
    ),
    (
        623,
        "Visit www.commonmark.org.\n\nVisit www.commonmark.org/a.b.\n",
        r#"para["Visit ",link("http://www.commonmark.org")["www.commonmark.org"],"."],para["Visit ",link("http://www.commonmark.org/a.b")["www.commonmark.org/a.b"],"."]"#,
    ),
    (
        624,
        "www.google.com/search?q=Markup+(business)\n\nwww.google.com/search?q=Markup+(business)))\n\n(www.google.com/search?q=Markup+(business))\n\n(www.google.com/search?q=Markup+(business)\n",
        r#"para[link("http://www.google.com/search?q=Markup+(business)")["www.google.com/search?q=Markup+(business)"]],para[link("http://www.google.com/search?q=Markup+(business)")["www.google.com/search?q=Markup+(business)"],"))"],para["(",link("http://www.google.com/search?q=Markup+(business)")["www.google.com/search?q=Markup+(business)"],")"],para["(",link("http://www.google.com/search?q=Markup+(business)")["www.google.com/search?q=Markup+(business)"]]"#,
    ),
    (
        625,
        "www.google.com/search?q=(business))+ok\n",
        r#"para[link("http://www.google.com/search?q=(business))+ok")["www.google.com/search?q=(business))+ok"]]"#,
    ),
    (
        626,
        "www.google.com/search?q=commonmark&hl=en\n\nwww.google.com/search?q=commonmark&hl;\n",
        r#"para[link("http://www.google.com/search?q=commonmark&hl=en")["www.google.com/search?q=commonmark&hl=en"]],para[link("http://www.google.com/search?q=commonmark")["www.google.com/search?q=commonmark"],"&hl;"]"#,
    ),
    (
        627,
        "www.commonmark.org/he<lp\n",
        r#"para[link("http://www.commonmark.org/he")["www.commonmark.org/he"],"<lp"]"#,
    ),
    (
        628,
        "http://commonmark.org\n\n(Visit https://encrypted.google.com/search?q=Markup+(business))\n\nAnonymous FTP is available at ftp://foo.bar.baz.\n",
        r#"para[link("http://commonmark.org")["http://commonmark.org"]],para["(Visit ",link("https://encrypted.google.com/search?q=Markup+(business)")["https://encrypted.google.com/search?q=Markup+(business)"],")"],para["Anonymous FTP is available at ",link("ftp://foo.bar.baz")["ftp://foo.bar.baz"],"."]"#,
    ),
    (
        629,
        "foo@bar.baz\n",
        r#"para[link("mailto:foo@bar.baz")["foo@bar.baz"]]"#,
    ),
    (
        630,
        "hello@mail+xyz.example isn't valid, but hello+xyz@mail.example is.\n",
        r#"para["hello@mail+xyz.example isn't valid, but ",link("mailto:hello+xyz@mail.example")["hello+xyz@mail.example"]," is."]"#,
    ),
    (
        631,
        "a.b-c_d@a.b\n\na.b-c_d@a.b.\n\na.b-c_d@a.b-\n\na.b-c_d@a.b_\n",
        r#"para[link("mailto:a.b-c_d@a.b")["a.b-c_d@a.b"]],para[link("mailto:a.b-c_d@a.b")["a.b-c_d@a.b"],"."],para["a.b-c_d@a.b-"],para["a.b-c_d@a.b_"]"#,
    ),
];

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn commonmarks_examples_of_blocks_are_read_to_the_trees_its_specification_gives() {
    let wrong = misread(COMMONMARK_BLOCKS);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn commonmarks_examples_of_inlines_are_read_to_the_trees_its_specification_gives() {
    let wrong = misread(COMMONMARK_INLINES);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn githubs_tables_tasks_strikethrough_and_bare_links_are_read_as_its_specification_says() {
    let mut cases = GFM.to_vec();
    cases.extend([
        // A table's columns align as its delimiter row says, and its pipes may go at neither end.
        (
            0,
            "a | b | c | d\n:-- | :-: | --: | ---\n1 | 2 | 3 | 4\n",
            r#"table(l,c,r,-)[head[cell["a"],cell["b"],cell["c"],cell["d"]],row[cell["1"],cell["2"],cell["3"],cell["4"]]]"#,
        ),
        // The lines of a paragraph above its header row stay a paragraph.
        (
            0,
            "words\n| a |\n| - |\n| 1 |\n",
            r#"para["words"],table(-)[head[cell["a"]],row[cell["1"]]]"#,
        ),
        // `~` struck as `~~` is, a run of three is no mark.
        (0, "~one~ ~~~two~~~\n", r#"para[del["one"]," ~~~two~~~"]"#),
        // A task of an ordered list, and one with nothing after its box.
        (
            0,
            "1. [X] done\n2. [ ]\n",
            r#"list(1,tight)[item(x)[para["done"]],item( )[]]"#,
        ),
        // No bare link inside a link, nor where a word goes on before it.
        (
            0,
            "[see www.a.org](/x) awww.b.org\n",
            r#"para[link("/x")["see www.a.org"]," awww.b.org"]"#,
        ),
    ]);
    let wrong = misread(&cases);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn html_in_the_text_is_kept_as_the_text_it_is_in_blocks_and_in_lines() {
    let wrong = misread(&[
        (
            0,
            "<div>\n*not emphasis*\n</div>\n",
            r#"html"<div>\n*not emphasis*\n</div>""#,
        ),
        (
            0,
            "<script>\nalert(1)\n\n</script>\nafter\n",
            r#"html"<script>\nalert(1)\n\n</script>",para["after"]"#,
        ),
        (
            0,
            "<img src=x onerror=alert(1)>\n",
            r#"html"<img src=x onerror=alert(1)>""#,
        ),
        (
            0,
            "A <b>bold</b> word, <!-- a comment --> and <a href=\"x\">a tag</a>.\n",
            r#"para["A ",html"<b>","bold",html"</b>"," word, ",html"<!-- a comment -->"," and ",html"<a href=\"x\">","a tag",html"</a>","."]"#,
        ),
        // What only looks like a tag is text.
        (0, "a <3 b and <x y=>\n", r#"para["a <3 b and <x y=>"]"#),
    ]);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn references_to_characters_are_the_characters_html_names_and_the_rest_is_text() {
    let wrong = misread(&[
        (
            0,
            "&amp; &lt; &gt; &quot; &copy; &hellip; &nbsp;x &Aring; &ThickSpace;.\n",
            "para[\"& < > \\\" © … \u{a0}x Å \u{205f}\u{200a}.\"]",
        ),
        (
            0,
            "&#35; &#x1F600; &#X22; &#0; &#xD800; &#1234567; &#12345678;\n",
            "para[\"# 😀 \\\" \u{fffd} \u{fffd} \u{fffd} &#12345678;\"]",
        ),
        (
            0,
            "&nosuchname; &amp &;\n",
            r#"para["&nosuchname; &amp &;"]"#,
        ),
        (
            0,
            "[a](/q?x=1&amp;y=2 \"&laquo;t&raquo;\")\n",
            r#"para[link("/q?x=1&y=2","«t»")["a"]]"#,
        ),
        (0, "```&lt;b&gt;\nx\n```\n", r#"code(<b>)"x""#),
        // In code, a reference is the text it is.
        (0, "`&amp;`\n", r#"para[code"&amp;"]"#),
    ]);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A script that says the blocks of `markdown` with their lines (`kind@line+lines[...]`).
fn lines_of(markdown: &str) -> String {
    let script = format!(
        r#"
        local markdown = require("@pencere/markdown")
        local function said(blocks)
            local out = {{}}
            for _, block in ipairs(blocks or {{}}) do
                local kids = (block.type == "list" or block.type == "item" or block.type == "quote")
                    and "[" .. said(block.children) .. "]" or ""
                out[#out + 1] = block.type .. "@" .. block.line .. "+" .. block.lines .. kids
            end
            return table.concat(out, ",")
        end
        local text = said(markdown.parse({}).children)
        return {{ render = function() return pencere.text(text) end }}
        "#,
        lua(markdown)
    );
    said(&script).join("")
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn every_block_says_the_lines_of_the_text_it_came_from() {
    let text = concat!(
        "# Title\n\nSome words\non two lines\n\n- one\n- two\n  more of two\n\n  > a quote\n\n",
        "```lua\nx = 1\n```\n\n[a]: /x\nWords after a definition\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n",
        "    code\n\n\n    indented\n\nUnder\n=====\n\n---\n",
    );
    assert_eq!(
        lines_of(text),
        "heading@1+1,paragraph@3+2,list@6+5[item@6+1[paragraph@6+1],item@7+4[paragraph@7+2,\
         quote@10+1[paragraph@10+1]]],code@12+3,paragraph@17+1,table@19+3,code@23+4,heading@28+2,rule@31+1"
    );
}

/// A page that keeps a document and says, after each change in `steps` (Lua that changes `text`),
/// how many lines and blocks the change was read in, and whether the document is the one the text
/// reads to all at once.
fn changes(setup: &str, steps: &[&str]) -> Vec<String> {
    let mut script = format!(
        "{DUMP}\nlocal markdown = require('@pencere/markdown')\nlocal said = {{}}\n{setup}\n\
         local doc = markdown.document()\ndoc:set(text)\n"
    );
    for step in steps {
        let _ = writeln!(
            script,
            "do\n{step}\nend\ndoc:set(text)\nlocal lines, blocks = doc:reread()\n\
             said[#said + 1] = lines .. \" \" .. blocks .. \" \" .. tostring(dump(doc:tree()) == dump(markdown.parse(text)))"
        );
    }
    script.push_str(
        "return { render = function() local root = pencere.div() for _, line in ipairs(said) do \
         root:child(pencere.text(line)) end return root end }",
    );
    said(&script)
}

const FIFTY: &str = r#"
    local parts = {}
    for i = 1, 50 do parts[#parts + 1] = "Paragraph " .. i .. "\nand more of it" end
    local text = table.concat(parts, "\n\n")
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_change_is_read_again_from_the_block_before_it_until_the_blocks_are_as_they_were() {
    let said = changes(
        FIFTY,
        &[
            // A word of the tenth paragraph: the blank line before it, its two lines, the blank
            // after it and the first line of the eleventh, where the blocks meet again.
            r#"text = string.gsub(text, "Paragraph 10\n", "Paragraph ten\n")"#,
            // A heading and a paragraph put in after the twentieth.
            r##"text = string.gsub(text, "Paragraph 20\n(.-)\n\n", "Paragraph 20\n%1\n\n# A heading\n\nnew words\n\n")"##,
            // And taken out again.
            r##"text = string.gsub(text, "# A heading\n\nnew words\n\n", "")"##,
            // A line that joins the paragraph above it (a lazy line of no block of its own).
            r#"text = string.gsub(text, "\n\nParagraph 30\n", "\nParagraph 30\n")"#,
            // An underline that makes a heading of the paragraph above it.
            r#"text = string.gsub(text, "Paragraph 40\nand more of it", "Paragraph 40\n===")"#,
            // The same text: nothing is read.
            "",
            // The last line: the last paragraph is read again.
            r#"text = text .. " at the end""#,
        ],
    );
    assert_eq!(
        said,
        [
            "5 1 true", "6 2 true", "2 0 true", "6 1 true", "4 1 true", "0 0 true", "2 1 true",
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_change_inside_a_fence_a_list_or_a_quote_reads_that_block_again_and_no_more() {
    let setup = r#"
        local parts = { "Before" }
        parts[#parts + 1] = "```lua"
        for i = 1, 20 do parts[#parts + 1] = "local x" .. i .. " = " .. i end
        parts[#parts + 1] = "```"
        parts[#parts + 1] = ""
        for i = 1, 10 do parts[#parts + 1] = "- item " .. i end
        parts[#parts + 1] = ""
        for i = 1, 10 do parts[#parts + 1] = "> quoted " .. i end
        parts[#parts + 1] = ""
        parts[#parts + 1] = "After"
        local text = table.concat(parts, "\n")
    "#;
    let said = changes(
        setup,
        &[
            // A line in the fence: its 22 lines are read, and the blank and the list's first line.
            r#"text = string.gsub(text, "local x10 = 10", "local x10 = 'ten'")"#,
            // An item: the list's 10 lines, the blank and the quote's first line.
            r#"text = string.gsub(text, "item 5", "item five")"#,
            // A line of the quote: its 10 lines, the blank and the last paragraph.
            r#"text = string.gsub(text, "quoted 7", "quoted seven")"#,
            // A fence closed early: the rest of it is a paragraph, and its old closing fence opens
            // one that takes all after it, so all after it is read.
            r#"text = string.gsub(text, "local x15 = 15", "```")"#,
        ],
    );
    assert_eq!(said, ["24 1 true", "12 1 true", "12 1 true", "46 3 true"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_change_to_a_tables_header_reads_again_the_paragraph_whose_line_it_was() {
    let said = changes(
        r#"local text = "Intro\n\nwords\n| a |\n| - |\n| 1 |\n\nAfter""#,
        &[
            // The delimiter row no more: the header is a line of the paragraph again.
            r#"text = string.gsub(text, "| %- |", "| x |")"#,
            // And again: the paragraph's last line is the header of a table.
            r#"text = string.gsub(text, "| x |", "| - |")"#,
            // A row: the table is read again with the paragraph it began in.
            r#"text = string.gsub(text, "| 1 |", "| one |")"#,
        ],
    );
    // Each reads from the paragraph to the line after the blank that ends the table.
    assert_eq!(said, ["6 1 true", "6 2 true", "6 2 true"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_definition_that_changes_reads_again_the_blocks_that_name_it_and_no_other() {
    let said = said(
        r#"
        local markdown = require("@pencere/markdown")
        local doc = markdown.document()
        local function href()
            return doc:tree().children[1].children[1].href
        end
        local text = "[a link][a]\n\nNo link here\n\n[a]: /one\n"
        doc:set(text)
        local said = { href() }
        local before = doc:render()
        doc:set(string.gsub(text, "/one", "/two"))
        local after = doc:render()
        local lines, blocks = doc:reread()
        said[#said + 1] = href()
        said[#said + 1] = lines .. " " .. blocks
        -- The paragraph that names no label is drawn as it was; the link's is drawn anew.
        said[#said + 1] = tostring(rawequal(before.kids[2], after.kids[2]))
        said[#said + 1] = tostring(rawequal(before.kids[1], after.kids[1]))
        -- A definition taken away leaves the brackets as text.
        doc:set("[a link][a]\n\nNo link here\n")
        said[#said + 1] = doc:tree().children[1].children[1].type
        return { render = function() return pencere.text(table.concat(said, " | ")) end }
        "#,
    );
    // The definition alone is read again (the blank line before it and its line), and the
    // paragraph that names it has its inlines read again, not its lines.
    assert_eq!(said, ["/one | /two | 2 1 | true | false | text"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_kept_document_draws_again_only_the_blocks_that_changed() {
    let said = said(&format!(
        r#"
        local markdown = require("@pencere/markdown")
        {FIFTY}
        local doc = markdown.document()
        doc:set(text)
        local before = doc:render()
        doc:set(string.gsub(text, "Paragraph 25\n", "Paragraph twenty-five\n"))
        local after = doc:render()
        local same = 0
        for index = 1, 50 do
            if rawequal(before.kids[index], after.kids[index]) then same += 1 end
        end
        -- Drawn with other options, every block is drawn anew.
        local other = doc:render({{ gap = 4 }})
        local kept = 0
        for index = 1, 50 do
            if rawequal(after.kids[index], other.kids[index]) then kept += 1 end
        end
        return {{ render = function() return pencere.text(same .. " " .. kept) end }}
        "#
    ));
    // The twenty-fifth alone is new.
    assert_eq!(said, ["49 0"]);
}

/// The words of a page's texts and texts of many looks (each one string), in order.
fn texts(root: &Element) -> Vec<String> {
    root.fold()
        .into_iter()
        .filter_map(|node| match node.kind {
            Kind::Text(text) => Some(text),
            Kind::Rich(spans) => Some(spans.iter().map(|span| span.text.as_str()).collect()),
            _ => None,
        })
        .collect()
}

/// A page made of `script` driven a frame at a time, each frame after a wake of its timers, until
/// it says "done": the words of each frame and how many elements it had. A call past the time a
/// call has at least (the floor's 250 ms) fails the page.
fn frames_until_done(script: &str) -> Vec<(Vec<String>, usize)> {
    let mut page = open_script(script, ModuleLimits::floor());
    let mut frames = Vec::new();
    let mut now = 0.0;
    for _ in 0..500 {
        page.module.render(SIZE, now, false);
        let update: Update = page.until(Duration::from_secs(20), |update| {
            update.frame.is_some() || update.closed.is_some()
        });
        assert_eq!(update.closed, None, "{}", failed(&update));
        let frame = update.frame.expect("a page");
        let said = texts(&frame.root);
        let done = said.iter().any(|word| word == "done");
        frames.push((said, frame.root.fold().len()));
        if done {
            return frames;
        }
        now += 0.02;
        page.module.wake(now);
    }
    panic!("the page never said it was done");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_large_text_is_read_a_step_at_a_time_and_is_busy_until_all_of_it_is_read() {
    let frames = frames_until_done(
        r###"
        local markdown = require("@pencere/markdown")
        local parts = {}
        for i = 1, 4000 do
            parts[#parts + 1] = "## Part " .. i .. "\n\nSome *words* of part " .. i .. ", [a link](https://example.com/" .. i .. ").\n"
        end
        local text = table.concat(parts, "\n")
        local doc = markdown.document()
        doc:set(text)
        return { render = function()
            local tree = doc:tree()
            local state = doc:busy() and "busy" or "done"
            return pencere.div()
                :child(pencere.text(state))
                :child(pencere.text("blocks " .. #tree.children))
                :child(pencere.text("bytes " .. #text))
        end }
        "###,
    );
    let (first, _) = &frames[0];
    assert_eq!(first[0], "busy");
    let (last, _) = frames.last().expect("a frame");
    assert_eq!(last[1], "blocks 8000");
    let bytes: usize = last[2]
        .strip_prefix("bytes ")
        .and_then(|bytes| bytes.parse().ok())
        .expect("its size");
    // What is read is shown as it comes, a step a call.
    let mut growing: Vec<usize> = frames
        .iter()
        .filter_map(|(said, _)| said[1].strip_prefix("blocks ")?.parse().ok())
        .collect();
    assert!(
        growing.windows(2).all(|pair| pair[0] <= pair[1]),
        "{growing:?}"
    );
    assert!(growing[0] > 0 && growing[0] < 400, "{growing:?}");
    growing.dedup();
    // A step reads some 20 000 bytes, its lines and blocks costing more besides: the text took a
    // call for some thousands of its bytes, and each call was within its time (the page fails
    // otherwise).
    assert!(
        growing.len() > bytes / 20_000 && growing.len() < bytes / 2_000,
        "{} steps for {bytes} bytes",
        growing.len()
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_document_of_five_thousand_blocks_is_listed_a_view_at_a_time_within_a_calls_time() {
    let frames = frames_until_done(
        r#"
        local markdown = require("@pencere/markdown")
        local parts = {}
        for i = 1, 5000 do
            parts[#parts + 1] = "Paragraph " .. i .. " has some *words* and a `span` in it."
        end
        local doc = markdown.document()
        doc:set(table.concat(parts, "\n\n"))
        return { render = function()
            return pencere.div():size_full()
                :child(pencere.text(doc:busy() and "busy" or "done"))
                :child(doc:list("preview"):size_full())
        end }
        "#,
    );
    let (last, nodes) = frames.last().expect("a frame");
    assert!(
        last.iter()
            .any(|word| word == "Paragraph 1 has some words and a span in it."),
        "{last:?}"
    );
    assert!(!last.iter().any(|word| word.starts_with("Paragraph 5000 ")));
    for (_, nodes) in &frames {
        assert!(*nodes < 2_000, "{nodes} elements");
    }
    assert!(*nodes > 20, "{nodes} elements");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_draw_holds_to_its_limit_of_elements_and_says_that_the_rest_is_left_out() {
    let said = said(
        r#"
        local markdown = require("@pencere/markdown")
        local parts = {}
        for i = 1, 3000 do parts[#parts + 1] = "- item " .. i end
        local tree = markdown.parse(table.concat(parts, "\n") .. "\n\nAfter the list")
        local function count(element)
            local total = 1
            for _, kid in ipairs(element.kids or {}) do total += count(kid) end
            return total
        end
        local function last_words(element)
            local kids = element.kids or {}
            if #kids == 0 then return element.text end
            return last_words(kids[#kids])
        end
        local all = markdown.render(tree, { limit = 100000 })
        local held = markdown.render(tree, { limit = 500 })
        return { render = function()
            return pencere.text(count(all) .. " " .. count(held) .. " " .. tostring(last_words(held)))
        end }
        "#,
    );
    let [line] = said.as_slice() else {
        panic!("{said:?}")
    };
    let counts: Vec<&str> = line.split(' ').collect();
    let all: usize = counts[0].parse().expect("a count");
    let held: usize = counts[1].parse().expect("a count");
    assert!(all > 12_000, "{line}");
    assert!((480..=510).contains(&held), "{line}");
    assert_eq!(counts[2], "…");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_draw_names_its_links_and_says_the_address_of_each() {
    let said = said(
        r#"
        local markdown = require("@pencere/markdown")
        local tree = markdown.parse("A [link](https://example.com/a) and <mailto:x@y.z>.\n\n![a picture](p.png)")
        local element = markdown.render(tree, { id = "notes" })
        local said = {}
        local function walk(node)
            for _, span in ipairs(node.spans or {}) do
                if span.link then
                    said[#said + 1] = span.link .. "=" .. tostring(markdown.href(span.link))
                end
            end
            for _, kid in ipairs(node.kids or {}) do walk(kid) end
        end
        walk(element)
        -- An image is its description, in the quiet colour.
        local picture = element.kids[2].spans[1]
        said[#said + 1] = picture.text .. ":" .. tostring(picture.color)
        said[#said + 1] = tostring(markdown.href("notes/link/99"))
        said[#said + 1] = tostring(markdown.href(5))
        return { render = function() return pencere.text(table.concat(said, " ")) end }
        "#,
    );
    assert_eq!(
        said,
        ["notes/link/1=https://example.com/a notes/link/2=mailto:x@y.z a picture:dim nil nil"]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn code_is_coloured_by_the_language_the_page_gives_and_drawn_by_the_page_where_it_says() {
    let said = said(
        r#"
        local markdown = require("@pencere/markdown")
        local Syntax = require("@pencere/syntax")
        local toy = Syntax.define {
            name = "toy",
            start = function() return {} end,
            token = function(stream)
                if stream:eat_space() then return nil end
                if stream:match("local") then return "keyword" end
                if stream:match("%d+") then return "number" end
                stream:next()
            end,
            styles = { keyword = { color = "accent", weight = "bold" }, number = { color = "warning" } },
        }
        local languages = {
            find = function(word) return word == "toy" and "toy" or nil end,
            load = function() return { name = "toy", syntax = toy } end,
        }
        local tree = markdown.parse("```toy\nlocal x = 42\n```\n\n```other\nlocal y\n```")
        local said = {}
        local coloured = markdown.render(tree, { languages = languages })
        for _, span in ipairs(coloured.kids[1].kids[1].spans) do
            said[#said + 1] = span.text .. ":" .. tostring(span.color) .. ":" .. tostring(span.weight)
        end
        -- A language the page does not know is drawn plain.
        said[#said + 1] = #coloured.kids[2].kids[1].spans
        -- With no languages given, `@pencere/languages` is looked for where the kit has it, and
        -- code is plain where it has not.
        local default = markdown.render(tree)
        said[#said + 1] = default.kids[1].kids[1].spans[1].text
        -- The page's own drawing of code.
        local drawn = markdown.render(tree, { code = function(language, text)
            return pencere.text(tostring(language) .. "|" .. text)
        end })
        said[#said + 1] = drawn.kids[1].text
        said[#said + 1] = drawn.kids[2].text
        return { render = function() return pencere.text(table.concat(said, " ")) end }
        "#,
    );
    assert_eq!(
        said,
        [
            "local:accent:700  x = :nil:nil 42:warning:nil 1 local x = 42 toy|local x = 42 other|local y"
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_options_of_markdown_are_checked_and_a_mistake_says_the_line_of_the_page() {
    let said = said(
        r#"
        local markdown = require("@pencere/markdown")
        local tree = markdown.parse("x")
        local said = {}
        local function try(fn)
            local ok, why = pcall(fn)
            said[#said + 1] = ok and "ok" or why
        end
        try(function() markdown.parse(5) end)
        try(function() markdown.render({}) end)
        try(function() markdown.render(tree, { nope = 1 }) end)
        try(function() markdown.render(tree, { colors = { sky = "accent" } }) end)
        try(function() markdown.render(tree, { link = "https://x" }) end)
        try(function() markdown.render(tree, { headings = { 20, "big" } }) end)
        try(function() markdown.document():set(nil) end)
        try(function() markdown.document():list(pencere.div()) end)
        try(function() markdown.render(tree, { colors = { link = "info" }, gap = 4, headings = { 30 } }) end)
        return { render = function()
            local root = pencere.div()
            for _, line in ipairs(said) do root:child(pencere.text(line)) end
            return root
        end }
        "#,
    );
    assert_eq!(
        said,
        [
            "script.lua:9: markdown.parse takes the text of a document, not a number",
            "script.lua:10: markdown.render takes a document (markdown.parse), then options",
            "script.lua:11: markdown has no option nope (id, headings, colors, gap, size, link, code, image, languages, limit)",
            "script.lua:12: markdown colours no sky (text, quiet, link, code, head, quote, rule)",
            "script.lua:13: the option link of markdown is a function, not a string",
            "script.lua:14: the headings of markdown are the sizes of its six levels, a list of numbers",
            "script.lua:15: set takes the text of the document, not a nil",
            "script.lua:16: list takes the name of the box of a long document (a string), then the options",
            "ok",
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_language_says_the_look_its_styles_give_a_token_by_its_name() {
    let said = said(
        r#"
        local Syntax = require("@pencere/syntax")
        local language = Syntax.define {
            name = "toy",
            start = function() return {} end,
            token = function(stream) stream:next() end,
            styles = { string = { color = "success" }, strong = { weight = 700 } },
        }
        local look = language:look("string.special strong")
        local said = { look.color .. " " .. look.weight }
        -- A copy: the page may change it.
        look.color = "error"
        said[#said + 1] = language:look("string").color
        said[#said + 1] = tostring(language:look("comment"))
        local ok, why = pcall(function() return language:look(5) end)
        said[#said + 1] = why
        return { render = function() return pencere.text(table.concat(said, " | ")) end }
        "#,
    );
    assert_eq!(
        said,
        [
            "success 700 | success | nil | script.lua:15: look takes the name of a token's style, not a number"
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_text_of_more_looks_than_a_text_may_hold_parts_is_held_to_the_parts_the_host_takes() {
    // A paragraph of a thousand looks and more, then most of a mebibyte of plain words: the parts
    // past the thousandth are cut to a size a part may hold, and no more of them than a text may
    // have (1024) are made, the rest left out.
    let update = first(
        r#"
        local markdown = require("@pencere/markdown")
        local inlines = {}
        for _ = 1, 1100 do
            inlines[#inlines + 1] = { type = "emphasis", children = { { type = "text", text = "a" } } }
            inlines[#inlines + 1] = { type = "text", text = " b " }
        end
        inlines[#inlines + 1] = { type = "text", text = string.rep("x", 900000) }
        local tree = { type = "document", children = {
            { type = "paragraph", line = 1, lines = 1, children = inlines },
        } }
        local element = markdown.render(tree)
        return { render = function()
            return pencere.div():size_full():child(element)
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{}", failed(&update));
}

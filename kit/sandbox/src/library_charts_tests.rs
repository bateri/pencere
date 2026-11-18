//! `@pencere/charts` of the kit's library, run by the interpreter: a chart's model in the points of its
//! box, the sample under a point, the kit's painter and its options, a page's own painter given the
//! same model, a drawing kept while only the pointer moves, and the errors it raises for what its
//! caller gave it. Heavy: the interpreter is compiled and run (`make check-heavy`).

use std::time::Duration;

use crate::ModuleLimits;
use crate::luau_tests::{failed, first, open_script, words};

/// What a page shows that runs `checks`, Lua with `charts` and `check(what, got, want)` at hand
/// (tables compared by what they hold, their keys in order): "ok", or each check that came out
/// otherwise.
fn checked(checks: &str) -> Vec<String> {
    let script = format!(
        r#"
local charts = require("@pencere/charts")
local failures = {{}}
local function show(value)
    if type(value) ~= "table" then
        return tostring(value)
    end
    local parts = {{}}
    for name, item in value do
        parts[#parts + 1] = tostring(name) .. "=" .. show(item)
    end
    table.sort(parts)
    return "{{" .. table.concat(parts, ", ") .. "}}"
end
local function check(what, got, want)
    if show(got) ~= show(want) then
        failures[#failures + 1] = what .. ": " .. show(got) .. ", not " .. show(want)
    end
end
{checks}
return {{ render = function()
    local root = pencere.div()
    if #failures == 0 then
        root:child(pencere.text("ok"))
    end
    for _, failure in ipairs(failures) do
        root:child(pencere.text(failure))
    end
    return root
end }}
"#
    );
    let mut page = open_script(&script, ModuleLimits::default());
    page.module.render(crate::test_support::SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    if let Some(closed) = &update.closed {
        panic!("the page closed: {closed:?}");
    }
    words(&update.frame.expect("a page").root)
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_charts_model_is_its_lines_in_the_points_of_its_box_the_newest_at_the_right() {
    let shown = checked(
        r#"
        local line = { { values = { 0, 50, 100 }, role = "accent" } }
        local m = charts.model({ w = 100, h = 50, count = 3, ceiling = 100, series = line })
        -- A pixel at the top and the foot is kept clear, so that a line along either is seen whole.
        check("points", m.series[1].points, { { 0, 49 }, { 50, 25 }, { 100, 1 } })
        check("a rule at the half, on the middle of a pixel", m.rules, { 25.5 })
        check("no marker", m.marker, nil)
        check("the box", { m.w, m.h }, { 100, 50 })
        local short = charts.model({ w = 100, h = 50, count = 5, ceiling = 100,
            series = { { values = { 10, 20 }, role = "red" } } })
        check("a short line starts from the right", short.series[1].points[1][1], 75)
        local one = charts.model({ w = 100, h = 50, count = 5, series = { { values = { 10 }, role = "red" } } })
        check("one value is no line", #one.series, 0)
        local zeros = charts.model({ w = 10, h = 10, series = { { values = { 0, 0, 0 }, role = "accent" } } })
        check("a chart of zeros has a scale", zeros.series[1].points[2][2], 9)
        local quiet = charts.model({ w = 10, h = 10, ceiling = 0, series = { { values = { 0, 0 }, role = "accent" } } })
        check("a ceiling of 0 is a scale of 1", quiet.series[1].points[2][2], 9)
        local scaled = charts.model({ w = 30, h = 12, series = { { values = { 1, 2, 4 }, role = "accent" } } })
        check("the ceiling is the largest value shown", scaled.series[1].points[3][2], 1)
        check("the count is the longest line's", scaled.series[1].points[1][1], 0)
        local over = charts.model({ w = 10, h = 12, count = 2, ceiling = 2, series = { { values = { -5, 9 }, role = "accent" } } })
        check("values past the scale stay in the box", { over.series[1].points[1][2], over.series[1].points[2][2] }, { 11, 1 })
        local ruled = charts.model({ w = 10, h = 20, rules = { 0, 1 }, series = line })
        check("rules where the page says", ruled.rules, { 0.5, 20.5 })
        local marked = charts.model({ w = 100, h = 50, count = 3, ceiling = 100, marker = 1, series = line })
        check("the marker", marked.marker, { x = 50, points = { { 50, 25, "accent" } } })
        local past = charts.model({ w = 100, h = 50, count = 3, ceiling = 100, marker = 3, series = line })
        check("a marker past the oldest sample is none", past.marker, nil)
        check("the newest", charts.back_at({ w = 100, count = 3 }, 100), 0)
        check("the oldest", charts.back_at({ w = 100, count = 3 }, 0), 2)
        check("the nearest", charts.back_at({ w = 100, count = 3 }, 70), 1)
        check("outside the box", charts.back_at({ w = 100, count = 3 }, 101), nil)
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_chart_is_painted_by_the_kits_painter_or_the_pages_and_kept_while_only_the_pointer_moves() {
    let shown = checked(
        r#"
        local line = { { values = { 0, 50, 100 }, role = "accent" } }
        local box = charts.area({ w = 100, h = 50, count = 3, ceiling = 100, series = line, label = "Use" })
        check("a box of its size", { box.style.width, box.style.height }, { 100, 50 })
        check("its words for a screen reader", box.meta.label, "Use")
        check("a canvas in it", box.kids[1].kind, "canvas")
        check("a rule, an area and a line", #box.kids[1].paths, 3)
        local marked = charts.area({ w = 100, h = 50, count = 3, ceiling = 100, series = line, marker = 0 })
        check("and the marker's line and dot", #marked.kids[1].paths, 5)

        local wide = charts.painter({ line = 3, fill = 0.5, ink = "dim", rule = 0.1, dot = 6 })
        local model = charts.model({ w = 10, h = 10, count = 2, marker = 0, series = { { values = { 1, 2 }, role = "accent" } } })
        local paths = wide.area(model)
        check("the rule's ink", paths[1].paint.stroke.color, pencere.alpha("dim", 0.1))
        check("the fill", paths[2].paint.fill, pencere.alpha("accent", 0.5))
        check("the line", paths[3].paint.stroke, { width = 3, color = "accent" })
        local dot = wide.marker(model)[2]
        check("the marker's dot", dot.segments[1], { "move", 7, -2 })

        -- A page's painter is given the model, and its drawing is kept by the chart's id.
        local drawn = 0
        local seen = nil
        local mine = {
            area = function(m) drawn += 1 seen = m return {} end,
            marker = function(m) return { pencere.path():rect(m.marker.x, 0, 1, 1):fill("red") } end,
        }
        local spec = { id = "cpu", w = 100, h = 50, count = 3, ceiling = 100, series = line, painter = mine }
        charts.area(spec)
        check("the model", seen.series[1].points[2], { 50, 25 })
        check("the area is drawn with no marker", seen.marker, nil)
        charts.area(spec)
        check("the same is drawn once", drawn, 1)
        spec.marker = 1
        local over = charts.area(spec)
        check("the pointer moves only the marker", drawn, 1)
        check("the marker is the painter's", #over.kids[1].paths, 1)
        spec.series = { { values = { 50, 100, 0 }, role = "accent" } }
        charts.area(spec)
        check("a new sample is drawn again", drawn, 2)
        spec.id = nil
        charts.area(spec)
        charts.area(spec)
        check("a chart with no id is drawn each time", drawn, 4)

        -- The drawings of this frame and the one before are kept; a chart not drawn since lets go.
        local drawn_a = 0
        local counted = { area = function(m) drawn_a += 1 return {} end, marker = mine.marker }
        local a = { id = "a", w = 10, h = 10, series = line, painter = counted }
        local b = { id = "b", w = 10, h = 10, series = line }
        charts.area(a)
        charts.area(b)
        charts.area(b)
        charts.area(a)
        check("a chart of the frame before is kept", drawn_a, 1)
        charts.area(b)
        charts.area(b)
        charts.area(b)
        charts.area(a)
        check("a chart not drawn for a frame is drawn again", drawn_a, 2)
        -- A page that names a chart anew each time keeps 64 of them at most.
        charts.area(a)
        for index = 1, 64 do
            charts.area({ id = "new " .. index, w = 10, h = 10, series = line })
        end
        charts.area({ id = "one more", w = 10, h = 10, series = line })
        for index = 1, 64 do
            charts.area({ id = "new again " .. index, w = 10, h = 10, series = line })
        end
        charts.area(a)
        check("past 64 a chart is let go", drawn_a, 3)

        -- A kit that does not have the module is asked without failing.
        check("a module that is not there", (pcall(require, "@pencere/nothing")), false)
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_chart_given_what_is_not_one_is_an_error_at_the_callers_line() {
    let line = "{ { values = { 1, 2 }, role = 'accent' } }";
    let cases = [
        (
            "charts.area(5)".to_owned(),
            "a chart is a table of what it is, not number",
        ),
        (
            format!("charts.area({{ series = {line} }})"),
            "a chart says its size, w and h, points above 0",
        ),
        (
            "charts.area({ w = 10, h = 10 })".to_owned(),
            "a chart's series is a list of { values =, role = }",
        ),
        (
            "charts.area({ w = 10, h = 10, series = { { values = { 1, 'x' }, role = 'accent' } } })"
                .to_owned(),
            "value 2 of series 1 of a chart is not a number",
        ),
        (
            "charts.area({ w = 10, h = 10, series = { { values = { 1 } } } })".to_owned(),
            "series 1 of a chart has no colour (role)",
        ),
        (
            format!("charts.model({{ w = 10, h = 10, count = 4001, series = {line} }})"),
            "a chart's count is a whole number of samples from 2 to 4000",
        ),
        (
            format!("charts.area({{ w = 10, h = 10, ceiling = 'top', series = {line} }})"),
            "a chart's ceiling is a number",
        ),
        (
            format!("charts.area({{ w = 10, h = 10, rules = {{ 2 }}, series = {line} }})"),
            "a chart's rule is a fraction of its height, from 0 to 1",
        ),
        (
            format!("charts.area({{ w = 10, h = 10, painter = {{}}, series = {line} }})"),
            "a chart's painter has area(model) and marker(model)",
        ),
        (
            "charts.painter({ line = -1 })".to_owned(),
            "a painter's line is a number from 0",
        ),
        (
            "charts.back_at({ w = 10 }, 3)".to_owned(),
            "back_at takes a chart's w and count",
        ),
    ];
    for (call, why) in cases {
        let script = format!("local charts = require('@pencere/charts')\n{call}\nreturn {{}}");
        let said = failed(&first(&script, ModuleLimits::default()));
        assert_eq!(said, format!("script.lua:2: {why}"), "{call}");
    }
}

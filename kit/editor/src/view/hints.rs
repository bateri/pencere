//! Text the page puts in a line that is not the document's (`Widget`: an inlay hint, a line's end
//! error, a blame): measured with the line (it takes room in it, so that where a character is, where
//! the caret is and what a click is on all go through [`Row`]'s places), drawn after the text, and
//! found under the pointer.
//!
//! A line with widgets is measured again when what they say or how wide they are changes, and not
//! otherwise: its measure is kept under the text and the widgets together.

use std::rc::Rc;

use pencere_view::edit::{Widget, WidgetGroup, WidgetShow, WidgetSide, limits};
use pencere_view::{Color, Font, Measure, Point, Quad, Rect, Role, Run};
use pencere_view::{EditorSlot, Paint};

use super::{Gap, Placed, Row, Shaped, View};
use crate::buffer::Buffer;

/// Where a widget is, in the content (pixels), and what it is.
#[derive(Clone, Debug)]
pub(crate) struct WidgetBox {
    pub rect: Rect,
    pub placed: Placed,
    /// The boxes of its parts, across, same order as the parts.
    pub parts: Vec<Rect>,
}

impl View {
    /// The options of the group a widget is in.
    fn group_of(&self, widget: &Widget) -> WidgetGroup {
        widget
            .group
            .as_ref()
            .and_then(|name| {
                self.options
                    .widgets
                    .iter()
                    .find(|(group, _)| group == name)
                    .map(|(_, group)| group.clone())
            })
            .unwrap_or_default()
    }

    /// Whether a widget shows, as its group says and the modifiers held are.
    fn widget_shown(&self, widget: &Widget) -> bool {
        let group = self.group_of(widget);
        let held = group.hold.control == self.mods.control
            && group.hold.alt == self.mods.alt
            && group.hold.command == self.mods.command
            && (!group.hold.shift || self.mods.shift);
        match group.show {
            WidgetShow::On => true,
            WidgetShow::Off => false,
            WidgetShow::UnlessHeld => !held,
            WidgetShow::WhenHeld => held,
        }
    }

    /// The widgets of line `line` that show, in the order they are at, with where in the line.
    fn line_widgets(&self, buffer: &Buffer, line: usize) -> Vec<(usize, Widget)> {
        if !buffer.has_widgets() {
            return Vec::new();
        }
        let (start, end) = (buffer.line_start(line), buffer.line_end(line));
        let mut out: Vec<(usize, usize, Widget)> = Vec::new();
        for (_, item) in buffer.decorations_in(start, end) {
            let pencere_view::edit::Mark::Widget(widget) = &item.mark else {
                continue;
            };
            if !self.widget_shown(widget) {
                continue;
            }
            let place = match widget.side {
                WidgetSide::Before | WidgetSide::End => item.from,
                WidgetSide::After => item.to,
            };
            if place < start || place > end {
                continue;
            }
            // The order the page wrote them in the same place.
            out.push((place - start, out.len(), widget.clone()));
        }
        out.sort_by_key(|(at, order, _)| (*at, *order));
        out.truncate(limits::WIDGETS_IN_LINE);
        out.into_iter()
            .map(|(at, _, widget)| (at, widget))
            .collect()
    }

    /// Line `line` measured with the room its widgets take.
    pub(super) fn shape_line(
        &self,
        buffer: &Buffer,
        line: usize,
        measure: &mut dyn Measure,
    ) -> Rc<Shaped> {
        let text = buffer.line(line);
        let widgets = self.line_widgets(buffer, line);
        if widgets.is_empty() {
            return self.shape(&text, measure);
        }
        let key = format!(
            "{text}\u{1}{widgets:?}{:?}{}",
            self.options.widgets, self.scale
        );
        if let Some(found) = self.shaped.borrow().get(&key) {
            return found.clone();
        }
        let base = self.shape(&text, measure);
        let boundary = |at: usize| {
            base.stops
                .partition_point(|stop| stop.0 <= at)
                .checked_sub(1)
                .and_then(|at| base.stops.get(at))
                .map_or(0, |stop| stop.0)
        };
        let mut placed = Vec::new();
        let mut gaps: Vec<Gap> = Vec::new();
        for (at, widget) in widgets {
            let at = boundary(at);
            let (width, parts) = self.widget_width(&widget, measure);
            if widget.side != WidgetSide::End {
                let at_gap = match gaps.iter().position(|gap| gap.at == at) {
                    Some(found) => found,
                    None => {
                        gaps.push(Gap {
                            at,
                            before: 0.0,
                            after: 0.0,
                        });
                        gaps.len() - 1
                    }
                };
                if let Some(gap) = gaps.get_mut(at_gap) {
                    if widget.side == WidgetSide::Before {
                        gap.before += width;
                    } else {
                        gap.after += width;
                    }
                }
            }
            placed.push(Placed {
                at,
                widget,
                width,
                parts,
            });
        }
        gaps.sort_by_key(|gap| gap.at);
        let stops = base
            .stops
            .iter()
            .map(|(chars, x)| {
                let added: f32 = gaps
                    .iter()
                    .filter(|gap| gap.at <= *chars)
                    .map(|gap| gap.before + gap.after)
                    .sum();
                (*chars, x + added)
            })
            .collect();
        let shaped = Rc::new(Shaped {
            stops,
            breaks: base.breaks.clone(),
            gaps,
            widgets: placed,
        });
        let mut kept = self.shaped.borrow_mut();
        if kept.len() >= super::SHAPED {
            kept.clear();
        }
        kept.insert(key, shaped.clone());
        shaped
    }

    /// The font a group's words are set in.
    fn widget_font(&self, group: &WidgetGroup) -> Font {
        let size = group
            .size
            .clamp(*limits::WIDGET_SIZE.start(), *limits::WIDGET_SIZE.end());
        Font {
            size: self.font.size * size,
            ..self.font
        }
    }

    /// How wide a widget is (pixels) and how wide each of its parts' words are.
    fn widget_width(&self, widget: &Widget, measure: &mut dyn Measure) -> (f32, Vec<f32>) {
        let group = self.group_of(widget);
        let font = self.widget_font(&group);
        let room = |points: Option<f32>| {
            points
                .unwrap_or(group.pad)
                .clamp(*limits::WIDGET_ROOM.start(), *limits::WIDGET_ROOM.end())
                * self.scale
        };
        let parts: Vec<f32> = widget
            .parts
            .iter()
            .take(limits::WIDGET_PARTS)
            .map(|part| {
                let text: String = part.text.chars().take(limits::WIDGET_TEXT).collect();
                if self.plain {
                    text.chars().count() as f32 * font.size * 0.6
                } else {
                    measure.measure(&font, &text).width
                }
            })
            .collect();
        let width = parts.iter().sum::<f32>() + room(widget.pad_left) + room(widget.pad_right);
        (width, parts)
    }

    /// Where the widgets of `row` are, in the content (pixels, across from the room's left edge,
    /// down from the top of the content), each with its parts.
    pub(crate) fn widget_boxes(&self, row: &Row) -> Vec<WidgetBox> {
        if row.shaped.widgets.is_empty() {
            return Vec::new();
        }
        let top = self.pad + self.y_of(row.index);
        let origin = self.text_x();
        let mut out = Vec::new();
        // Each boundary's before-widgets from where the characters before it end, then the caret, then
        // its after-widgets.
        let mut before: Vec<(usize, f32)> = Vec::new();
        let mut after: Vec<(usize, f32)> = Vec::new();
        for placed in &row.shaped.widgets {
            let at = placed.at;
            let inside = at >= row.from && (at < row.to || row.last);
            let x = match placed.widget.side {
                WidgetSide::Before => {
                    let used = take(&mut before, at);
                    row.end_x(at) + used
                }
                WidgetSide::After => {
                    let used = take(&mut after, at);
                    row.caret_x(at) + used
                }
                WidgetSide::End => {
                    if !row.last {
                        continue;
                    }
                    let used = take(&mut after, usize::MAX);
                    row.shaped.width() - row.shift
                        + self.group_of(&placed.widget).gap * self.scale
                        + used
                }
            };
            if placed.widget.side == WidgetSide::End {
                before.clear();
            } else if !inside {
                continue;
            }
            let width = placed.width;
            let side = placed.widget.side;
            match side {
                WidgetSide::Before => before.push((at, width)),
                WidgetSide::After => after.push((at, width)),
                WidgetSide::End => after.push((usize::MAX, width + self.space)),
            }
            let group = self.group_of(&placed.widget);
            let pad_left = placed
                .widget
                .pad_left
                .unwrap_or(group.pad)
                .clamp(*limits::WIDGET_ROOM.start(), *limits::WIDGET_ROOM.end())
                * self.scale;
            let rect = Rect::new(origin + x, top, width, self.row);
            let mut across = rect.x + pad_left;
            let mut parts = Vec::new();
            for part in &placed.parts {
                parts.push(Rect::new(across, top, *part, self.row));
                across += part;
            }
            out.push(WidgetBox {
                rect,
                placed: placed.clone(),
                parts,
            });
        }
        out
    }

    /// What is under `point` (pixels of the content): a widget, and the part of it.
    pub(crate) fn widget_under(&self, point: Point) -> Option<(WidgetBox, Option<usize>)> {
        let row = self.rows.iter().find(|row| {
            let top = self.pad + self.y_of(row.index);
            point.y >= top && point.y < top + self.row
        })?;
        self.widget_boxes(row).into_iter().find_map(|found| {
            if point.x < found.rect.x || point.x >= found.rect.right() {
                return None;
            }
            let part = found
                .parts
                .iter()
                .position(|part| point.x >= part.x && point.x < part.right());
            Some((found, part))
        })
    }

    /// The widgets of `row` drawn: each a ground and its words, the last at the end of its line cut
    /// where the room ends.
    pub(super) fn paint_widgets(
        &self,
        row: &Row,
        (origin_x, y): (f32, f32),
        slot: &EditorSlot,
        clip: Rect,
        out: &mut Vec<Paint>,
    ) {
        let room_right = origin_x - self.text_x() + self.room.w - self.pad;
        for found in self.widget_boxes(row) {
            let widget = &found.placed.widget;
            let group = self.group_of(widget);
            let font = self.widget_font(&group);
            // Back from the content's pixels to the page's (the text's origin and the row's top).
            let (dx, dy) = (
                origin_x - self.text_x(),
                y - (self.pad + self.y_of(row.index)),
            );
            let rect = found.rect.translate(dx, dy);
            let fade = |color: Color| color.faded(slot.opacity);
            let ground = widget
                .background
                .or(group.background)
                .or(Some(Role::Foreground.alpha(0.08)));
            let height = (self.line * 0.95).round().min(self.row);
            let top = rect.y + (self.row - height) / 2.0;
            let boxed = Rect::new(rect.x, top, rect.w, height);
            let end = widget.side == WidgetSide::End;
            if let (Some(ground), false) = (ground, end) {
                out.push(Paint::Quad(Quad {
                    rect: boxed,
                    radii: [group.radius * self.scale; 4],
                    fill: Some(fade(ground)),
                    gradient: None,
                    border: None,
                    shadow: None,
                    clip,
                }));
            }
            let text_y =
                rect.y + self.lead + (self.line - self.line * (font.size / self.font.size)) / 2.0;
            let base = widget.color.or(group.color).or(Some(Role::Dim.solid()));
            for (at, part) in widget.parts.iter().take(limits::WIDGET_PARTS).enumerate() {
                let Some(span) = found.parts.get(at) else {
                    continue;
                };
                let span = span.translate(dx, dy);
                let (Some(color), true) = (part.color.or(base), span.x < room_right + self.pad)
                else {
                    continue;
                };
                let mut text: String = part.text.chars().take(limits::WIDGET_TEXT).collect();
                // A widget at a line's end goes as far as the room does, and ends in "…".
                if end && span.right() > room_right {
                    let each = span.w / text.chars().count().max(1) as f32;
                    let fit = ((room_right - span.x) / each.max(0.1)).floor() as usize;
                    text = text.chars().take(fit.saturating_sub(1)).collect::<String>() + "…";
                }
                let mut run_font = font;
                if let Some(weight) = part.weight {
                    run_font.weight = weight;
                }
                if let Some(italic) = part.italic {
                    run_font.italic = italic;
                }
                if let Some(back) = part.background {
                    out.push(Paint::Quad(Quad {
                        rect: Rect::new(span.x, top, span.w, height),
                        radii: [group.radius * self.scale; 4],
                        fill: Some(fade(back)),
                        gradient: None,
                        border: None,
                        shadow: None,
                        clip,
                    }));
                }
                out.push(Paint::Text(Run {
                    origin: Point::new(span.x, text_y),
                    text,
                    font: run_font,
                    color,
                    opacity: slot.opacity,
                    grounds: ground.into_iter().collect(),
                    clip,
                    underline: false,
                    strike: false,
                }));
            }
        }
    }
}

/// What has been put at `at` already, and taken: the room before the next widget there.
fn take(used: &mut [(usize, f32)], at: usize) -> f32 {
    used.iter()
        .filter(|(place, _)| *place == at)
        .map(|(_, w)| w)
        .sum()
}

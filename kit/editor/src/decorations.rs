//! The page's decorations, by layer, carried over every change of the document; and which of the
//! regions that fold are folded.

use pencere_view::edit::{Decoration, Mark, Refusal, Zone, limits};

use crate::changes::{Assoc, Step, map};

/// One layer: its name and its decorations, in order of where they start.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layer {
    pub name: String,
    pub items: Vec<Decoration>,
    /// Whether any of the items is a mark of the overview ruler: the layers that hold none are not
    /// looked through for them every frame.
    pub rulers: bool,
    /// The same for the gaps kept open for the page.
    pub zones: bool,
    /// The same for the names of ranges.
    pub anchors: bool,
    /// The same for the looks that are drawn by the modifiers held or the pointer's place.
    pub gated: bool,
    /// The same for widgets.
    pub widgets: bool,
    /// How long the longest of the items is: how far back from a place an item that reaches it can
    /// start, so that a look at the lines in view does not go through all that come before them.
    pub longest: usize,
}

impl Layer {
    /// What is known of the items said again, after they changed.
    fn sum_up(&mut self) {
        self.longest = self
            .items
            .iter()
            .map(|item| item.to.saturating_sub(item.from))
            .max()
            .unwrap_or(0);
    }
}

/// An editor's decorations, and its folded regions.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Decorations {
    pub layers: Vec<Layer>,
    /// The regions folded: each the range of a fold decoration, in order.
    pub folded: Vec<(usize, usize)>,
}

impl Decorations {
    /// The layer `name`'s decorations that overlap `from..to` (or, empty, stand in it) put in place
    /// of by `items`; refused, with nothing changed, where the editor would hold more than it may.
    pub fn replace(
        &mut self,
        name: &str,
        from: usize,
        to: usize,
        mut items: Vec<Decoration>,
    ) -> Result<(), Refusal> {
        let at = self.layers.iter().position(|layer| layer.name == name);
        if at.is_none() && items.is_empty() {
            return Ok(());
        }
        if at.is_none() && self.layers.len() >= limits::LAYERS {
            return Err(Refusal::TooManyDecorations);
        }
        let others: usize = self
            .layers
            .iter()
            .filter(|layer| layer.name != name)
            .map(|layer| layer.items.len())
            .sum();
        let kept: Vec<Decoration> = at
            .and_then(|at| self.layers.get(at))
            .map(|layer| {
                layer
                    .items
                    .iter()
                    .filter(|item| !in_range(item, from, to))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if others + kept.len() + items.len() > limits::DECORATIONS {
            return Err(Refusal::TooManyDecorations);
        }
        let named = |items: &[Decoration]| {
            items
                .iter()
                .filter(|item| {
                    matches!(&item.mark, Mark::Anchor(_))
                        || matches!(&item.mark, Mark::Widget(widget) if widget.anchor.is_some())
                })
                .count()
        };
        let other_names: usize = self
            .layers
            .iter()
            .filter(|layer| layer.name != name)
            .map(|layer| named(&layer.items))
            .sum();
        if other_names + named(&kept) + named(&items) > limits::ANCHORS {
            return Err(Refusal::TooManyDecorations);
        }
        let mut all = kept;
        all.append(&mut items);
        all.sort_by_key(|item| (item.from, item.to));
        let rulers = all.iter().any(|item| matches!(item.mark, Mark::Ruler(_)));
        let zones = all.iter().any(|item| matches!(item.mark, Mark::Zone(_)));
        let anchors = all.iter().any(|item| matches!(item.mark, Mark::Anchor(_)));
        let gated = all.iter().any(
            |item| matches!(&item.mark, Mark::Style(look) if look.held.is_some() || look.over || look.cursor.is_some()),
        );
        let widgets = all.iter().any(|item| matches!(item.mark, Mark::Widget(_)));
        match at.and_then(|at| self.layers.get_mut(at)) {
            Some(layer) => {
                layer.items = all;
                layer.rulers = rulers;
                layer.zones = zones;
                layer.anchors = anchors;
                layer.gated = gated;
                layer.widgets = widgets;
                layer.sum_up();
            }
            None => {
                let mut layer = Layer {
                    name: name.to_owned(),
                    items: all,
                    rulers,
                    zones,
                    anchors,
                    gated,
                    widgets,
                    longest: 0,
                };
                layer.sum_up();
                self.layers.push(layer);
            }
        }
        self.layers.retain(|layer| !layer.items.is_empty());
        Ok(())
    }

    /// Every decoration carried over `steps`: a range's start after what comes in at it, its end
    /// before; one whose characters all went goes too (a mark of a place stays at it). The folded
    /// regions are carried the same way.
    pub fn map(&mut self, steps: &[Step]) {
        if steps.is_empty() {
            return;
        }
        for layer in &mut self.layers {
            let mut items = std::mem::take(&mut layer.items);
            items.retain_mut(|item| {
                let was_empty = item.from == item.to;
                if was_empty {
                    let at = map(steps, item.from, Assoc::Before);
                    item.from = at;
                    item.to = at;
                    return true;
                }
                let from = map(steps, item.from, Assoc::After);
                let to = map(steps, item.to, Assoc::Before);
                item.from = from;
                item.to = to;
                from < to
            });
            items.sort_by_key(|item| (item.from, item.to));
            layer.items = items;
            layer.sum_up();
        }
        self.layers.retain(|layer| !layer.items.is_empty());
        let mut folded = std::mem::take(&mut self.folded);
        folded.retain_mut(|(from, to)| {
            *from = map(steps, *from, Assoc::After);
            *to = map(steps, *to, Assoc::Before);
            from < to
        });
        self.folded = folded;
    }

    /// The decorations of every layer that overlap `from..to`, with the layer each is of (its
    /// place in the order the layers came).
    pub fn in_range(&self, from: usize, to: usize) -> Vec<(usize, &Decoration)> {
        let mut out = Vec::new();
        for (index, layer) in self.layers.iter().enumerate() {
            // The items are in order of their starts: those starting at or past `to` are not it,
            // apart from a mark of the place `to` when the range is empty, and those that start
            // further back than the longest of them reach are over before `from`.
            let end = layer.items.partition_point(|item| item.from <= to);
            let start = layer
                .items
                .partition_point(|item| item.from.saturating_add(layer.longest) < from);
            for item in layer.items.get(start..end).unwrap_or_default() {
                let overlaps = if item.from == item.to {
                    item.from >= from && item.from <= to
                } else {
                    item.to > from && item.from < to.max(from + 1)
                };
                if overlaps {
                    out.push((index, item));
                }
            }
        }
        out
    }

    /// The marks of the overview ruler, of every layer, in order, with the place of the layer.
    pub fn rulers(&self) -> Vec<(usize, &Decoration)> {
        self.layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| layer.rulers)
            .flat_map(|(index, layer)| {
                layer
                    .items
                    .iter()
                    .filter(|item| matches!(item.mark, Mark::Ruler(_)))
                    .map(move |item| (index, item))
            })
            .collect()
    }

    /// The gaps kept open for the page, of every layer, in order of the place they are at.
    pub fn zones(&self) -> Vec<(usize, Zone)> {
        let mut out: Vec<(usize, Zone)> = self
            .layers
            .iter()
            .filter(|layer| layer.zones)
            .flat_map(|layer| layer.items.iter())
            .filter_map(|item| match item.mark {
                Mark::Zone(zone) => Some((item.from, zone)),
                _ => None,
            })
            .collect();
        out.sort_by_key(|(at, _)| *at);
        out
    }

    /// The widgets that name their box, by name, with the place each is at (the range's start, or
    /// its end for one after a character) and its side.
    pub fn widget_anchors(&self) -> Vec<(String, usize, pencere_view::edit::WidgetSide)> {
        let mut out = Vec::new();
        for layer in self.layers.iter().filter(|layer| layer.widgets) {
            for item in &layer.items {
                let Mark::Widget(widget) = &item.mark else {
                    continue;
                };
                let Some(name) = &widget.anchor else {
                    continue;
                };
                let place = match widget.side {
                    pencere_view::edit::WidgetSide::After => item.to,
                    _ => item.from,
                };
                out.push((name.clone(), place, widget.side));
            }
        }
        out
    }

    /// Whether any layer holds a widget.
    pub fn has_widgets(&self) -> bool {
        self.layers.iter().any(|layer| layer.widgets)
    }

    /// Whether any look is drawn by the modifiers held or the pointer's place.
    pub fn gated(&self) -> bool {
        self.layers.iter().any(|layer| layer.gated)
    }

    /// The ranges the page named, by name, each with its range: of a name said more than once, the
    /// one in the layer that came last (and, in it, the one that starts last).
    pub fn anchors(&self) -> Vec<(String, usize, usize)> {
        let mut out: Vec<(String, usize, usize)> = Vec::new();
        for layer in self.layers.iter().filter(|layer| layer.anchors) {
            for item in &layer.items {
                let Mark::Anchor(name) = &item.mark else {
                    continue;
                };
                match out.iter_mut().find(|(known, _, _)| known == name) {
                    Some(found) => *found = (name.clone(), item.from, item.to),
                    None => out.push((name.clone(), item.from, item.to)),
                }
            }
        }
        out
    }

    /// The regions that can be folded: the fold decorations of every layer, in order.
    pub fn folds(&self) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = self
            .layers
            .iter()
            .flat_map(|layer| layer.items.iter())
            .filter(|item| item.mark == Mark::Fold && item.from < item.to)
            .map(|item| (item.from, item.to))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The folded regions that are still regions that fold, after the page changed the decorations
    /// (`line_of` the line of a place): one stays folded where a fold starts on the same line it
    /// did, with that fold's range, or, as it is, where it starts on one of `others` (the lines the
    /// editor folds from of itself).
    pub fn keep_folded(&mut self, line_of: &dyn Fn(usize) -> usize, others: &[usize]) {
        let mut folded = std::mem::take(&mut self.folded);
        // Asked of every layer's every change, with no folded region as with thousands.
        let folds: Vec<(usize, (usize, usize))> = if folded.is_empty() {
            Vec::new()
        } else {
            self.folds()
                .into_iter()
                .map(|fold| (line_of(fold.0), fold))
                .collect()
        };
        folded.retain_mut(|region| {
            let line = line_of(region.0);
            // In order of where they start, so in order of their lines: the first on the line.
            let first = folds.partition_point(|(on, _)| *on < line);
            match folds.get(first).filter(|(on, _)| *on == line) {
                Some((_, fold)) => {
                    *region = *fold;
                    true
                }
                None => others.contains(&line),
            }
        });
        folded.sort_unstable();
        folded.dedup();
        self.folded = folded;
    }
}

/// Whether a decoration is one of those `from..to` takes the place of: one that overlaps the range,
/// or a mark of a place inside it (of the place itself, for an empty range).
fn in_range(item: &Decoration, from: usize, to: usize) -> bool {
    if item.from == item.to {
        item.from >= from && (item.from < to || from == to && item.from == to)
    } else {
        item.from < to && item.to > from
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `in_range` is to say of `from..to`, every item looked at.
    fn looked_at_all(
        decorations: &Decorations,
        from: usize,
        to: usize,
    ) -> Vec<(usize, Decoration)> {
        let mut out = Vec::new();
        for (index, layer) in decorations.layers.iter().enumerate() {
            for item in &layer.items {
                let overlaps = if item.from == item.to {
                    item.from >= from && item.from <= to
                } else {
                    item.to > from && item.from < to.max(from + 1)
                };
                if overlaps {
                    out.push((index, item.clone()));
                }
            }
        }
        out
    }

    fn found(decorations: &Decorations, from: usize, to: usize) -> Vec<(usize, Decoration)> {
        decorations
            .in_range(from, to)
            .into_iter()
            .map(|(index, item)| (index, item.clone()))
            .collect()
    }

    /// Short items, one long one that starts far back, marks of a place, and each carried over a
    /// change: whatever the longest is and however it came to be, nothing that reaches a range is
    /// left out, nothing else is said, and the order stays.
    #[test]
    fn a_range_finds_what_reaches_it_however_far_back_it_starts() {
        let mut decorations = Decorations::default();
        let short: Vec<Decoration> = (0..200)
            .map(|n| Decoration::new(n * 10, n * 10 + 3, Mark::Fold))
            .collect();
        let marks: Vec<Decoration> = (0..50)
            .map(|n| Decoration::new(n * 37, n * 37, Mark::Fold))
            .collect();
        decorations
            .replace("short", 0, usize::MAX, short)
            .expect("held");
        decorations
            .replace("marks", 0, usize::MAX, marks)
            .expect("held");
        decorations
            .replace(
                "long",
                0,
                usize::MAX,
                vec![
                    Decoration::new(5, 1900, Mark::Fold),
                    Decoration::new(1000, 1004, Mark::Fold),
                ],
            )
            .expect("held");
        let check = |decorations: &Decorations| {
            for from in (0..2100).step_by(7) {
                for length in [0, 1, 3, 40] {
                    assert_eq!(
                        found(decorations, from, from + length),
                        looked_at_all(decorations, from, from + length),
                        "{from}..{}",
                        from + length
                    );
                }
            }
        };
        check(&decorations);
        // The long one goes and the rest are looked for from nearer by.
        decorations.replace("long", 0, 6, Vec::new()).expect("held");
        assert_eq!(
            decorations
                .layers
                .iter()
                .find(|layer| layer.name == "long")
                .map(|layer| layer.longest),
            Some(4)
        );
        check(&decorations);
        // An edit makes an item longer: what comes in inside it is in it.
        decorations.map(&[Step {
            from: 1001,
            to: 1001,
            len: 500,
        }]);
        assert_eq!(
            decorations
                .layers
                .iter()
                .find(|layer| layer.name == "long")
                .map(|layer| layer.longest),
            Some(504)
        );
        check(&decorations);
    }

    /// A folded region is told by the line it starts on, as many as there are: the first fold on the
    /// line is what it is now, and one on no fold's line goes unless the editor folds from there.
    #[test]
    fn folded_regions_stay_with_the_folds_on_their_lines() {
        let line_of = |at: usize| at / 10;
        let mut decorations = Decorations::default();
        decorations
            .replace(
                "folds",
                0,
                usize::MAX,
                vec![
                    Decoration::new(11, 40, Mark::Fold),
                    Decoration::new(15, 18, Mark::Fold),
                    Decoration::new(50, 70, Mark::Fold),
                ],
            )
            .expect("held");
        decorations.folded = vec![(12, 30), (51, 60), (80, 90), (95, 99)];
        decorations.keep_folded(&line_of, &[9]);
        assert_eq!(decorations.folded, vec![(11, 40), (50, 70), (95, 99)]);
    }

    #[test]
    fn an_editor_holds_no_more_layers_and_decorations_than_it_may() {
        let mut decorations = Decorations::default();
        let mark = |at: usize| Decoration::new(at, at + 1, Mark::Fold);
        for layer in 0..limits::LAYERS {
            let added = decorations.replace(&format!("layer {layer}"), 0, 10, vec![mark(0)]);
            assert_eq!(added, Ok(()));
        }
        assert_eq!(
            decorations.replace("one too many", 0, 10, vec![mark(0)]),
            Err(Refusal::TooManyDecorations)
        );
        // A layer there takes the place of its own.
        assert_eq!(decorations.replace("layer 0", 0, 10, vec![mark(1)]), Ok(()));
        let many: Vec<Decoration> = (0..limits::DECORATIONS).map(mark).collect();
        assert_eq!(
            decorations.replace("layer 1", 0, limits::DECORATIONS + 1, many),
            Err(Refusal::TooManyDecorations)
        );
    }
}

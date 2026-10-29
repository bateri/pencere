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
            }
            None => self.layers.push(Layer {
                name: name.to_owned(),
                items: all,
                rulers,
                zones,
                anchors,
                gated,
                widgets,
            }),
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
            // apart from a mark of the place `to` when the range is empty.
            let end = layer.items.partition_point(|item| item.from <= to);
            for item in layer.items.get(..end).unwrap_or_default() {
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
        let folds = self.folds();
        let mut folded = std::mem::take(&mut self.folded);
        folded.retain_mut(|region| {
            let line = line_of(region.0);
            match folds.iter().find(|fold| line_of(fold.0) == line) {
                Some(fold) => {
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

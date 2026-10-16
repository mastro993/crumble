//! Sidebar row drag reordering: the floating row, the live drop order, and the rows' slide
//! motion. Copied from SkillBinder's sidebar; Crumble saves the order at once, so it drops
//! SkillBinder's wait for a save.

use std::{
    cell::Cell,
    collections::HashMap,
    hash::Hash,
    time::{Duration, Instant},
};

use super::account_dot::check;
use super::sidebar_row::row_with;
use crumble_store::AccountColor;
use crumble_theme::sidebar::{ROW_GAP, ROW_HEIGHT};
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{
    App, Bounds, Context, CursorStyle, IntoElement, Pixels, Point, Render, SharedString, Styled,
    Window, px,
};

/// How long a row takes to slide into its new slot.
const SHIFT: Duration = Duration::from_millis(180);
/// Distance between the tops of neighbouring rows.
const PITCH: f32 = ROW_HEIGHT + ROW_GAP;

/// Drag payload for reordering rows; also what the floating row shows.
pub(super) struct RowDrag<Id> {
    pub(super) id: Id,
    pub(super) color: AccountColor,
    /// The Account is not Hidden.
    pub(super) checked: bool,
    pub(super) label: SharedString,
    pub(super) width: f32,
    /// Pointer distance below the row's top edge when the drag began.
    pub(super) grab: Cell<f32>,
}

impl<Id> RowDrag<Id> {
    /// Top of the floating row relative to the first slot of the list in `bounds`.
    pub(super) fn top(&self, position: Point<Pixels>, bounds: Bounds<Pixels>) -> f32 {
        f32::from(position.y - bounds.top()) - self.grab.get()
    }
}

/// The dragged row, floating under the pointer with a shadow.
pub(super) struct RowDragPreview {
    color: AccountColor,
    checked: bool,
    label: SharedString,
    width: f32,
}

impl RowDragPreview {
    pub(super) fn new<Id>(drag: &RowDrag<Id>) -> Self {
        Self {
            color: drag.color,
            checked: drag.checked,
            label: drag.label.clone(),
            width: drag.width,
        }
    }
}

impl Render for RowDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        row_with(
            "row-drag-preview",
            check(self.color, self.checked, cx),
            self.label.clone(),
            true,
            cx,
        )
        .w(px(self.width))
        .shadow_lg()
    }
}

/// The drag starts with the row's pointer; GPUI assigns the drag after the preview is built, so
/// the first move swaps in the grabbing hand.
pub(super) fn grab_cursor(window: &mut Window, cx: &mut App) {
    if cx.active_drag_cursor_style() != Some(CursorStyle::ClosedHand) {
        cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
    }
}

/// Drag and motion state for one list of rows.
pub(super) struct RowReorder<Id> {
    /// The dragged row and the index it takes if dropped now.
    dragging: Option<(Id, usize)>,
    /// Top of the floating row, relative to the first slot.
    floating_top: f32,
    /// Each moving row's displacement when the current motion started.
    shifts: HashMap<Id, f32>,
    started: Option<Instant>,
}

impl<Id> Default for RowReorder<Id> {
    fn default() -> Self {
        Self {
            dragging: None,
            floating_top: 0.0,
            shifts: HashMap::new(),
            started: None,
        }
    }
}

impl<Id: Clone + Eq + Hash> RowReorder<Id> {
    /// The stored order with the dragged row in the slot it would drop into.
    fn shown(&self, stored: &[Id]) -> Vec<Id> {
        match &self.dragging {
            Some((id, target)) => moved(stored.to_vec(), id, *target).unwrap_or(stored.to_vec()),
            None => stored.to_vec(),
        }
    }

    /// The order to draw, and the row floating under the pointer while a drag is active.
    pub(super) fn displayed(&self, stored: &[Id], drag_active: bool) -> (Vec<Id>, Option<Id>) {
        match &self.dragging {
            Some((id, _)) if drag_active => (self.shown(stored), Some(id.clone())),
            _ => (stored.to_vec(), None),
        }
    }

    /// Slides each row from its slot in `from` to its slot in `to`, continuing any motion
    /// still running.
    fn shift(&mut self, from: &[Id], to: &[Id], now: Instant, animate: bool) {
        let shifts = to.iter().enumerate().filter_map(|(slot, id)| {
            let old = from.iter().position(|candidate| candidate == id)?;
            let shift = (old as f32 - slot as f32) * PITCH + self.offset(id, now);
            (shift != 0.0).then(|| (id.clone(), shift))
        });
        self.shifts = if animate {
            shifts.collect()
        } else {
            HashMap::new()
        };
        self.started = Some(now);
    }

    fn progress(&self, now: Instant) -> f32 {
        self.started.map_or(1.0, |started| {
            let elapsed = now.saturating_duration_since(started).as_secs_f32();
            ease_out_cubic((elapsed / SHIFT.as_secs_f32()).min(1.0))
        })
    }

    /// How far the row is drawn from its slot at `now`.
    pub(super) fn offset(&self, id: &Id, now: Instant) -> f32 {
        self.shifts
            .get(id)
            .map_or(0.0, |shift| shift * (1.0 - self.progress(now)))
    }

    /// Rows are still sliding, so the next frame must be drawn.
    pub(super) fn moving(&self, now: Instant) -> bool {
        !self.shifts.is_empty() && self.progress(now) < 1.0
    }

    /// Follows the floating row, moving the other rows aside when it crosses a slot. Returns
    /// whether the drop slot changed.
    pub(super) fn drag(&mut self, stored: &[Id], id: &Id, top: f32, animate: bool) -> bool {
        let len = stored.len();
        let slot = ((top + ROW_HEIGHT / 2.0) / PITCH).floor();
        let dragging = Some((
            id.clone(),
            slot.clamp(0.0, len.saturating_sub(1) as f32) as usize,
        ));
        self.floating_top = top;
        if len == 0 || self.dragging == dragging {
            return false;
        }
        let before = self.shown(stored);
        self.dragging = dragging;
        let after = self.shown(stored);
        self.shift(&before, &after, Instant::now(), animate);
        true
    }

    /// Ends a drag: a release keeps the order shown and the floating row settles into its
    /// slot; a drag GPUI already ended elsewhere restores the stored order. Returns the order
    /// to save.
    pub(super) fn finish(
        &mut self,
        stored: &[Id],
        released: bool,
        animate: bool,
    ) -> Option<Vec<Id>> {
        let (id, _) = self.dragging.clone()?;
        let order = if released {
            self.shown(stored)
        } else {
            stored.to_vec()
        };
        let floating_top = self.floating_top;
        let save = self.reorder(stored, order.clone(), animate);
        if released
            && animate
            && let Some(slot) = order.iter().position(|other| other == &id)
        {
            self.shifts.insert(id, floating_top - slot as f32 * PITCH);
        }
        save
    }

    /// Moves `id` to `target`, sliding the rows it passes. Returns the order to save.
    pub(super) fn move_to(
        &mut self,
        stored: &[Id],
        id: &Id,
        target: usize,
        animate: bool,
    ) -> Option<Vec<Id>> {
        let order = moved(stored.to_vec(), id, target)?;
        self.reorder(stored, order, animate)
    }

    /// Slides from the order shown to `order`. Returns `order` when it differs from `stored`.
    fn reorder(&mut self, stored: &[Id], order: Vec<Id>, animate: bool) -> Option<Vec<Id>> {
        let shown = self.shown(stored);
        self.dragging = None;
        self.shift(&shown, &order, Instant::now(), animate);
        (order != stored).then_some(order)
    }
}

/// `order` with `id` moved to `target`, or `None` when nothing would change.
fn moved<Id: Eq>(mut order: Vec<Id>, id: &Id, target: usize) -> Option<Vec<Id>> {
    let from = order.iter().position(|candidate| candidate == id)?;
    let target = target.min(order.len() - 1);
    if from == target {
        return None;
    }
    let id = order.remove(from);
    order.insert(target, id);
    Some(order)
}

#[cfg(test)]
mod tests {
    use super::{PITCH, RowReorder, SHIFT, moved};
    use std::time::Instant;

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn dropping_on_a_row_takes_its_place() {
        let order = ids(&["a", "b", "c", "d"]);
        let id = |name: &str| name.to_owned();
        assert_eq!(
            moved(order.clone(), &id("a"), 2),
            Some(ids(&["b", "c", "a", "d"]))
        );
        assert_eq!(
            moved(order.clone(), &id("d"), 1),
            Some(ids(&["a", "d", "b", "c"]))
        );
        assert_eq!(
            moved(order.clone(), &id("b"), 9),
            Some(ids(&["a", "c", "d", "b"]))
        );
        assert_eq!(moved(order.clone(), &id("c"), 2), None);
        assert_eq!(moved(order, &id("x"), 0), None);
    }

    #[test]
    fn rows_slide_from_their_old_slot_to_the_new_one() {
        let mut reorder = RowReorder::default();
        let start = Instant::now();
        reorder.shift(&ids(&["a", "b", "c"]), &ids(&["b", "a", "c"]), start, true);
        let (a, b, c) = ("a".to_owned(), "b".to_owned(), "c".to_owned());
        assert_eq!(reorder.offset(&a, start), -PITCH);
        assert_eq!(reorder.offset(&b, start), PITCH);
        assert_eq!(reorder.offset(&c, start), 0.0);
        assert!(reorder.moving(start));
        assert_eq!(reorder.offset(&a, start + SHIFT), 0.0);
        assert!(!reorder.moving(start + SHIFT));

        // Reversing mid-motion continues from where each row is drawn.
        let midway = start + SHIFT / 2;
        let drawn = reorder.offset(&a, midway);
        reorder.shift(&ids(&["b", "a", "c"]), &ids(&["a", "b", "c"]), midway, true);
        assert_eq!(reorder.offset(&a, midway), PITCH + drawn);
    }

    #[test]
    fn a_drag_shows_the_drop_order_and_a_release_returns_it() {
        let mut reorder = RowReorder::default();
        let stored = ids(&["a", "b", "c"]);
        let c = "c".to_owned();
        assert!(reorder.drag(&stored, &c, 0.0, false));
        assert_eq!(reorder.displayed(&stored, true).0, ids(&["c", "a", "b"]));
        assert_eq!(reorder.displayed(&stored, true).1, Some(c.clone()));
        assert_eq!(
            reorder.finish(&stored, true, false),
            Some(ids(&["c", "a", "b"]))
        );
        assert_eq!(reorder.displayed(&stored, false), (stored.clone(), None));

        // A drag GPUI ended elsewhere saves nothing.
        assert!(reorder.drag(&stored, &c, 0.0, false));
        assert_eq!(reorder.finish(&stored, false, false), None);
        assert_eq!(reorder.move_to(&stored, &c, 2, false), None, "already last");
    }
}

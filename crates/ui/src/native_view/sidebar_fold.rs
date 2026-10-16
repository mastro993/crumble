//! Foldable sidebar sections: the title that folds them, its turning chevron, and the slide of
//! their rows. Copied from SkillBinder's sidebar.

use std::{f32::consts::FRAC_PI_2, time::Duration};

use super::NativeView;
use super::controls::app_icon;
use super::sidebar_row::activate;
use crumble_store::SidebarSection;
use crumble_theme::sidebar::{
    DETAIL_SIZE, ROW_GAP, ROW_HEIGHT, ROW_PADDING, SECTION_GAP, SECTION_HEIGHT,
};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, Context, Div, ElementId, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, div, px, radians,
};

const FOLD_MOTION: Duration = Duration::from_millis(180);

impl NativeView {
    /// Shows or hides a section's rows and remembers the choice across restarts.
    pub(super) fn toggle_section(&mut self, section: SidebarSection, cx: &mut Context<Self>) {
        if !self.store.folded_sections.remove(&section) {
            self.store.folded_sections.insert(section);
        }
        let motion = self.section_motion.entry(section).or_default();
        *motion += 1;
        let motion = *motion;
        // Folding rows stay mounted until they finish sliding shut.
        if cx.reduce_motion() {
            self.sections_moving.remove(&section);
        } else {
            self.sections_moving.insert(section);
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(FOLD_MOTION).await;
                let _ = this.update(cx, |view, cx| {
                    if view.section_motion.get(&section) == Some(&motion) {
                        view.sections_moving.remove(&section);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        self.store.save();
        cx.notify();
    }

    /// A section title that folds the section's rows.
    pub(super) fn section_header(
        &self,
        section: SidebarSection,
        title: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        let color = cx.theme().muted_foreground.opacity(0.8);
        let folded = self.store.folded_sections.contains(&section);
        let toggle = activate(
            div()
                .id(format!("section-{title}"))
                .flex()
                .flex_1()
                .items_center()
                .gap(px(4.0))
                .h_full()
                .cursor_pointer()
                .aria_expanded(!folded)
                .child(title)
                .child(self.section_chevron(section, cx)),
            title,
            cx,
            move |view, _, cx| view.toggle_section(section, cx),
        );
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(SECTION_HEIGHT))
            .mt(px(SECTION_GAP))
            .px(px(ROW_PADDING))
            .text_size(px(DETAIL_SIZE))
            .text_color(color)
            .child(toggle)
    }

    /// A chevron pointing down while the section is open, turning right as it folds.
    fn section_chevron(&self, section: SidebarSection, cx: &Context<Self>) -> AnyElement {
        let folded = self.store.folded_sections.contains(&section);
        let icon = app_icon("chevron-down")
            .size(px(DETAIL_SIZE))
            .text_color(cx.theme().muted_foreground.opacity(0.8));
        let turned = |folding: f32| radians(-FRAC_PI_2 * folding);
        match self.motion(section) {
            None => icon
                .rotate(turned(f32::from(u8::from(folded))))
                .into_any_element(),
            Some(motion) => icon
                .with_animation(
                    ElementId::NamedInteger(format!("{section:?}-chevron").into(), motion),
                    fold_animation(),
                    move |icon, delta| icon.rotate(turned(progress(folded, delta))),
                )
                .into_any_element(),
        }
    }

    /// A section's rows, sliding open or shut while a toggle is in motion; `items` is the number
    /// of rows.
    pub(super) fn section_rows(
        &self,
        section: SidebarSection,
        items: usize,
        rows: impl IntoElement + 'static,
    ) -> Option<AnyElement> {
        let folded = self.store.folded_sections.contains(&section);
        let Some(motion) = self.motion(section) else {
            return (!folded).then(|| rows.into_any_element());
        };
        // ponytail: slides toward a bound one gap per row above the rows' height, so the slide
        // ends a few points early; measure the rows if that ever shows.
        let height = items as f32 * (ROW_HEIGHT + ROW_GAP);
        let slide = div().overflow_hidden().child(rows).with_animation(
            ElementId::NamedInteger(format!("{section:?}-rows").into(), motion),
            fold_animation(),
            move |rows, delta| rows.max_h(px(height * (1.0 - progress(folded, delta)))),
        );
        Some(slide.into_any_element())
    }

    /// The toggle count keying a section's running slide.
    fn motion(&self, section: SidebarSection) -> Option<u64> {
        self.sections_moving
            .contains(&section)
            .then(|| self.section_motion.get(&section).copied())
            .flatten()
    }
}

fn fold_animation() -> Animation {
    Animation::new(FOLD_MOTION).with_easing(gpui_kit::base::animation::ease_in_out_cubic)
}

/// How far a section is folded `delta` of the way through its slide toward `folded`.
fn progress(folded: bool, delta: f32) -> f32 {
    if folded { delta } else { 1.0 - delta }
}

#[cfg(test)]
mod tests {
    use super::progress;

    #[test]
    fn progress_runs_toward_the_target_state() {
        assert_eq!(progress(true, 0.0), 0.0);
        assert_eq!(progress(true, 1.0), 1.0);
        assert_eq!(progress(false, 0.0), 1.0);
        assert_eq!(progress(false, 1.0), 0.0);
    }
}

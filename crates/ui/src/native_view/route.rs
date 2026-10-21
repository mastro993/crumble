//! Navigable routes and the in-memory back/forward history between them.

/// A rail destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Route {
    Home,
    Agenda,
    Calendar,
    Chat,
    Settings,
}

impl Route {
    pub(super) fn title(&self) -> &'static str {
        match self {
            Route::Home => "Home",
            Route::Agenda => "Agenda",
            Route::Calendar => "Calendar",
            Route::Chat => "Chat",
            Route::Settings => "Settings",
        }
    }

    /// Whether the page has a second-level sidebar, which the sidebar toggle shows and hides.
    pub(super) fn has_sidebar(&self) -> bool {
        *self == Route::Agenda
    }

    /// Whether the page runs to the panel's edges, as a page with a column of its own does,
    /// instead of sitting inside the panel's padding.
    pub(super) fn fills_panel(&self) -> bool {
        matches!(self, Route::Agenda | Route::Settings)
    }
}

/// Routes behind and ahead of the current one.
#[derive(Default)]
pub(super) struct History {
    back: Vec<Route>,
    forward: Vec<Route>,
}

impl History {
    /// Records leaving `from` for a new destination, which discards the forward routes.
    pub(super) fn push(&mut self, from: Route) {
        self.back.push(from);
        self.forward.clear();
    }

    pub(super) fn back(&mut self, current: Route) -> Option<Route> {
        let route = self.back.pop()?;
        self.forward.push(current);
        Some(route)
    }

    pub(super) fn forward(&mut self, current: Route) -> Option<Route> {
        let route = self.forward.pop()?;
        self.back.push(current);
        Some(route)
    }

    pub(super) fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub(super) fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{History, Route};

    #[test]
    fn back_and_forward_walk_the_visited_routes() {
        let mut history = History::default();
        history.push(Route::Home);
        history.push(Route::Calendar);
        assert_eq!(history.back(Route::Settings), Some(Route::Calendar));
        assert_eq!(history.back(Route::Calendar), Some(Route::Home));
        assert_eq!(history.back(Route::Home), None);
        assert_eq!(history.forward(Route::Home), Some(Route::Calendar));
        assert_eq!(history.forward(Route::Calendar), Some(Route::Settings));
        assert!(!history.can_go_forward());
    }

    #[test]
    fn navigating_after_going_back_drops_the_forward_routes() {
        let mut history = History::default();
        history.push(Route::Home);
        history.back(Route::Chat);
        assert!(history.can_go_forward());
        history.push(Route::Home);
        assert!(!history.can_go_forward());
        assert!(history.can_go_back());
    }
}

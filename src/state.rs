use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationItem {
    pub id: String,
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub icon_rgba: Option<Vec<u8>>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandVisualState {
    Idle,
    Compact,
    Expanded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerStatus {
    Starting,
    Listening,
    Paused,
    AccessDenied(String),
    Error(String),
}

pub struct IslandState {
    pub active: Option<NotificationItem>,
    pub queue: VecDeque<NotificationItem>,
    pub visual: IslandVisualState,
    pub paused: bool,
    pub auto_hide: bool,
    pub hovered: bool,
    collapse_deadline: Option<Instant>,
    collapse_timeout: Duration,
    pub listener_status: ListenerStatus,
}

impl IslandState {
    pub fn new(auto_hide: bool, collapse_timeout: Duration) -> Self {
        Self {
            active: None,
            queue: VecDeque::new(),
            visual: IslandVisualState::Idle,
            paused: false,
            auto_hide,
            hovered: false,
            collapse_deadline: None,
            collapse_timeout,
            listener_status: ListenerStatus::Starting,
        }
    }

    pub fn push_notification(&mut self, item: NotificationItem, now: Instant) {
        if self.paused {
            return;
        }
        if let Some(active) = &mut self.active
            && active.id == item.id
        {
            *active = item;
            if !self.hovered {
                self.collapse_deadline = Some(now + self.collapse_timeout);
            }
            return;
        }
        if let Some(existing) = self.queue.iter_mut().find(|q| q.id == item.id) {
            *existing = item;
            return;
        }

        if self.active.is_none() {
            self.activate(item, now);
        } else {
            self.queue.push_back(item);
        }
    }

    pub fn dismiss_active(&mut self, now: Instant) -> Option<String> {
        let dismissed = self.active.take().map(|n| n.id);
        if let Some(next) = self.queue.pop_front() {
            self.activate(next, now);
        } else {
            self.visual = IslandVisualState::Idle;
            self.collapse_deadline = None;
        }
        dismissed
    }

    pub fn remove_by_id(&mut self, id: &str, now: Instant) {
        if self.active.as_ref().is_some_and(|n| n.id == id) {
            self.dismiss_active(now);
            return;
        }
        self.queue.retain(|n| n.id != id);
    }

    pub fn set_hovered(&mut self, hovered: bool, now: Instant) {
        if self.hovered == hovered {
            return;
        }
        self.hovered = hovered;
        if self.active.is_some() {
            if hovered {
                self.visual = IslandVisualState::Expanded;
                self.collapse_deadline = None;
            } else {
                self.visual = IslandVisualState::Compact;
                self.collapse_deadline = Some(now + self.collapse_timeout);
            }
        }
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        if !self.hovered
            && let Some(deadline) = self.collapse_deadline
            && now >= deadline
        {
            self.dismiss_active(now);
            return true;
        }
        false
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            self.active = None;
            self.queue.clear();
            self.visual = IslandVisualState::Idle;
            self.collapse_deadline = None;
            if !matches!(
                self.listener_status,
                ListenerStatus::AccessDenied(_) | ListenerStatus::Error(_)
            ) {
                self.listener_status = ListenerStatus::Paused;
            }
        } else if matches!(self.listener_status, ListenerStatus::Paused) {
            self.listener_status = ListenerStatus::Listening;
        }
    }

    pub fn queued_count(&self) -> usize {
        self.queue.len()
    }

    pub fn should_be_visible(&self) -> bool {
        !self.auto_hide
            || self.active.is_some()
            || matches!(self.listener_status, ListenerStatus::AccessDenied(_))
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.collapse_deadline
    }

    fn activate(&mut self, item: NotificationItem, now: Instant) {
        self.active = Some(item);
        if self.hovered {
            self.visual = IslandVisualState::Expanded;
            self.collapse_deadline = None;
        } else {
            self.visual = IslandVisualState::Compact;
            self.collapse_deadline = Some(now + self.collapse_timeout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, title: &str) -> NotificationItem {
        NotificationItem {
            id: id.to_owned(),
            app_name: "TestApp".to_owned(),
            title: title.to_owned(),
            body: format!("Body for {title}"),
            icon_rgba: None,
            timestamp: 1_700_000_000,
        }
    }

    #[test]
    fn push_transitions_idle_to_compact_and_queues_subsequent() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));

        state.push_notification(sample("1", "First"), t0);
        assert_eq!(state.visual, IslandVisualState::Compact);
        assert_eq!(state.active.as_ref().map(|n| n.id.as_str()), Some("1"));
        assert_eq!(state.queued_count(), 0);

        state.push_notification(sample("2", "Second"), t0);
        assert_eq!(state.queued_count(), 1);

        // Dedup updates existing item instead of duplicating.
        state.push_notification(sample("2", "Second Updated"), t0);
        assert_eq!(state.queued_count(), 1);
        assert_eq!(state.queue[0].title, "Second Updated");
    }

    #[test]
    fn hover_expands_and_pauses_timeout_until_unhover() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        state.push_notification(sample("1", "Ping"), t0);

        state.set_hovered(true, t0 + Duration::from_secs(2));
        assert_eq!(state.visual, IslandVisualState::Expanded);
        assert_eq!(state.next_deadline(), None);
        assert!(!state.tick(t0 + Duration::from_secs(10)));

        let unhover_at = t0 + Duration::from_secs(10);
        state.set_hovered(false, unhover_at);
        assert_eq!(state.visual, IslandVisualState::Compact);
        assert!(!state.tick(unhover_at + Duration::from_secs(4)));
        assert!(state.tick(unhover_at + Duration::from_secs(5)));
        assert_eq!(state.visual, IslandVisualState::Idle);
    }

    #[test]
    fn dismiss_advances_queue_and_returns_id() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        state.push_notification(sample("1", "One"), t0);
        state.push_notification(sample("2", "Two"), t0);

        let dismissed = state.dismiss_active(t0 + Duration::from_secs(1));
        assert_eq!(dismissed.as_deref(), Some("1"));
        assert_eq!(state.active.as_ref().map(|n| n.id.as_str()), Some("2"));
        assert_eq!(state.visual, IslandVisualState::Compact);

        let dismissed_last = state.dismiss_active(t0 + Duration::from_secs(2));
        assert_eq!(dismissed_last.as_deref(), Some("2"));
        assert_eq!(state.active, None);
        assert_eq!(state.visual, IslandVisualState::Idle);
    }

    #[test]
    fn paused_ignores_incoming_and_auto_hide_visibility_works() {
        let t0 = Instant::now();
        let mut state = IslandState::new(true, Duration::from_secs(5));
        assert!(!state.should_be_visible());

        state.push_notification(sample("1", "Visible"), t0);
        assert!(state.should_be_visible());

        state.set_paused(true);
        assert_eq!(state.active, None);
        assert!(!state.should_be_visible());

        state.push_notification(sample("2", "Ignored"), t0);
        assert_eq!(state.active, None);

        state.listener_status = ListenerStatus::AccessDenied("denied".to_owned());
        assert!(state.should_be_visible());
    }
}

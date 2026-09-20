//! Opt-in device activity tracking from WhatsApp presence.
//!
//! The tracker records only what WhatsApp already exposes for a subscribed
//! chat: online state, last-seen timestamps, and typing indicators. Anything
//! hidden by the contact's privacy settings, or never delivered, stays
//! [`ActivityState::Unknown`]. Monitoring is session-only: it never starts by
//! itself and stops when the target is cleared. No polling, no bypasses.

/// How many timeline entries are kept per monitored number.
pub const MAX_HISTORY: usize = 50;

/// Minimum digits for a monitorable phone number, like the new-contact dialog.
pub const MIN_DIGITS: usize = 7;

/// Observable activity state of a monitored number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActivityState {
    Active,
    Inactive,
    #[default]
    Unknown,
}

impl ActivityState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Inactive => "Inactive",
            Self::Unknown => "Unknown",
        }
    }

    /// Honest one-line meaning. Inactive explicitly includes privacy-hiding
    /// because WhatsApp reports those contacts as offline.
    pub fn detail(self) -> &'static str {
        match self {
            Self::Active => "Online now, as reported by WhatsApp.",
            Self::Inactive => "Offline, or hidden by their privacy settings.",
            Self::Unknown => "No presence data yet. Start monitoring and wait for an update.",
        }
    }
}

/// One timeline entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityEventKind {
    Started,
    BecameActive,
    BecameInactive,
    TypingSeen,
    Stopped,
}

impl ActivityEventKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Started => "Monitoring started",
            Self::BecameActive => "Became active",
            Self::BecameInactive => "Became inactive",
            Self::TypingSeen => "Typing",
            Self::Stopped => "Monitoring stopped",
        }
    }
}

/// Timestamped timeline entry (Unix seconds, like the archive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivityEvent {
    pub at: i64,
    pub kind: ActivityEventKind,
}

/// Number under observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitoredTarget {
    /// Canonical chat id (`<digits>@s.whatsapp.net`).
    pub id: String,
    /// Display name: address-book name when known, else formatted number.
    pub label: String,
}

/// Session-only activity monitor. The views read this; [`crate::app::App`]
/// feeds it from presence and typing events.
#[derive(Clone, Debug, Default)]
pub struct ActivityTracker {
    /// Raw number input from the monitoring screen.
    pub input: String,
    pub target: Option<MonitoredTarget>,
    pub notify_on_change: bool,
    pub history: Vec<ActivityEvent>,
    pub last_state: ActivityState,
    pub last_change_at: Option<i64>,
    pub last_typing_at: Option<i64>,
}

impl ActivityTracker {
    pub fn monitoring(&self) -> bool {
        self.target.is_some()
    }

    /// Starts monitoring a target. Returns false when already monitoring it.
    pub fn start(&mut self, target: MonitoredTarget, now: i64) -> bool {
        if self.target.as_ref() == Some(&target) {
            return false;
        }
        self.target = Some(target);
        self.last_state = ActivityState::Unknown;
        self.last_change_at = None;
        self.last_typing_at = None;
        self.history.clear();
        self.push(ActivityEventKind::Started, now);
        true
    }

    /// Stops monitoring and clears the session state.
    pub fn stop(&mut self) {
        self.target = None;
        self.history.clear();
        self.last_state = ActivityState::Unknown;
        self.last_change_at = None;
        self.last_typing_at = None;
    }

    /// Records a presence update. Returns the new state when the monitored
    /// target changed state (for timeline and notification callers).
    pub fn note_presence(&mut self, id: &str, online: bool, now: i64) -> Option<ActivityState> {
        if self.target.as_ref().is_some_and(|target| target.id != id) {
            return None;
        }
        if self.target.is_none() {
            return None;
        }
        let state = if online {
            ActivityState::Active
        } else {
            ActivityState::Inactive
        };
        if state == self.last_state {
            return None;
        }
        self.last_state = state;
        self.last_change_at = Some(now);
        self.push(
            if online {
                ActivityEventKind::BecameActive
            } else {
                ActivityEventKind::BecameInactive
            },
            now,
        );
        Some(state)
    }

    /// Records a typing burst without changing the state. Consecutive bursts
    /// collapse into one entry so the timeline does not spam per keystroke.
    pub fn note_typing(&mut self, id: &str, now: i64) {
        if self.target.as_ref().is_none_or(|target| target.id != id) {
            return;
        }
        self.last_typing_at = Some(now);
        let duplicate = matches!(
            self.history.last(),
            Some(ActivityEvent {
                kind: ActivityEventKind::TypingSeen,
                ..
            })
        );
        if !duplicate {
            self.push(ActivityEventKind::TypingSeen, now);
        }
    }

    fn push(&mut self, kind: ActivityEventKind, now: i64) {
        self.history.push(ActivityEvent { at: now, kind });
        while self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }
}

/// Parses user-entered input into a monitorable chat id. Accepts formatted
/// numbers (`+39 333 123 4567`) and keeps the new-contact minimum of 7 digits.
pub fn parse_monitor_target(input: &str) -> Option<String> {
    let digits: String = input.chars().filter(char::is_ascii_digit).collect();
    (digits.len() >= MIN_DIGITS).then(|| format!("{digits}@s.whatsapp.net"))
}

/// Display label for a target: the known name, else the formatted number.
pub fn label_for(id: &str, known: Option<&str>) -> String {
    if let Some(name) = known.filter(|name| !name.trim().is_empty()) {
        return name.to_owned();
    }
    match crate::model::phone_of(id) {
        Some(digits) => crate::util::phone(digits),
        None => id.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_parse_like_new_contacts() {
        assert_eq!(
            parse_monitor_target("+39 333 123 4567"),
            Some("393331234567@s.whatsapp.net".into())
        );
        assert_eq!(
            parse_monitor_target("393331234567"),
            Some("393331234567@s.whatsapp.net".into())
        );
        assert_eq!(parse_monitor_target("123"), None);
        assert_eq!(parse_monitor_target(""), None);
        assert_eq!(parse_monitor_target("abcdefg"), None);
    }

    #[test]
    fn labels_prefer_names_then_numbers() {
        assert_eq!(label_for("1@s.whatsapp.net", Some("Ada")), "Ada");
        assert_eq!(label_for("1@s.whatsapp.net", Some("  ")), "+1");
        assert_eq!(label_for("1@s.whatsapp.net", None), "+1");
        assert_eq!(label_for("1-2@g.us", None), "1-2@g.us");
    }

    fn target() -> MonitoredTarget {
        MonitoredTarget {
            id: "393331234567@s.whatsapp.net".into(),
            label: "+39 333 123 456 7".into(),
        }
    }

    #[test]
    fn presence_drives_state_and_history_once_per_change() {
        let mut tracker = ActivityTracker::default();
        assert!(!tracker.monitoring());
        assert!(tracker.start(target(), 100));
        assert!(tracker.monitoring());
        assert_eq!(tracker.last_state, ActivityState::Unknown);
        // Restarting the same target is a no-op.
        assert!(!tracker.start(target(), 101));
        assert_eq!(tracker.history.len(), 1);

        assert_eq!(
            tracker.note_presence(&target().id, true, 110),
            Some(ActivityState::Active)
        );
        // A repeated online report adds nothing.
        assert_eq!(tracker.note_presence(&target().id, true, 120), None);
        assert_eq!(
            tracker.note_presence(&target().id, false, 130),
            Some(ActivityState::Inactive)
        );
        assert_eq!(tracker.last_change_at, Some(130));
        let kinds: Vec<_> = tracker.history.iter().map(|event| event.kind).collect();
        assert_eq!(
            kinds,
            [
                ActivityEventKind::Started,
                ActivityEventKind::BecameActive,
                ActivityEventKind::BecameInactive
            ]
        );
        // Other contacts never leak into the timeline.
        assert_eq!(tracker.note_presence("999@s.whatsapp.net", true, 140), None);
        assert_eq!(tracker.history.len(), 3);
    }

    #[test]
    fn typing_updates_last_activity_without_changing_state() {
        let mut tracker = ActivityTracker::default();
        tracker.note_typing(&target().id, 100);
        assert_eq!(tracker.last_typing_at, None);
        tracker.start(target(), 100);
        tracker.note_typing(&target().id, 110);
        tracker.note_typing(&target().id, 111);
        assert_eq!(tracker.last_typing_at, Some(111));
        assert_eq!(tracker.last_state, ActivityState::Unknown);
        let typing = tracker
            .history
            .iter()
            .filter(|event| event.kind == ActivityEventKind::TypingSeen)
            .count();
        assert_eq!(typing, 1, "consecutive bursts collapse into one entry");
    }

    #[test]
    fn history_is_capped_and_stop_clears_the_session() {
        let mut tracker = ActivityTracker::default();
        tracker.start(target(), 0);
        for now in 1..=(MAX_HISTORY as i64 + 10) {
            tracker.note_typing("other@s.whatsapp.net", now);
            tracker.note_presence(&target().id, now % 2 == 0, now);
        }
        assert_eq!(tracker.history.len(), MAX_HISTORY);
        tracker.stop();
        assert!(!tracker.monitoring());
        assert!(tracker.history.is_empty());
        assert_eq!(tracker.last_state, ActivityState::Unknown);
    }
}

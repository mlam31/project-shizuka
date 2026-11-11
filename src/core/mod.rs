use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Daily study goal (15 minutes) that keeps a streak alive.
pub const DAILY_GOAL_SECONDS: u64 = 15 * 60;

/// Consecutive days on which the daily study goal was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Streak {
    /// Number of consecutive qualified days.
    pub days: u32,
    /// Most recent day on which the goal was reached.
    pub last_qualified_day: Option<NaiveDate>,
}

impl Streak {
    /// Returns the streak as shown to the user.
    ///
    /// A streak stays alive when the last qualified day is today or yesterday;
    /// missing a full day breaks it.
    pub fn display_days(&self, today: NaiveDate) -> u32 {
        match self.last_qualified_day {
            Some(day) if day == today || day == today - Duration::days(1) => self.days,
            _ => 0,
        }
    }

    /// Marks `today` as qualified exactly once.
    ///
    /// Consecutive days extend the streak; any gap restarts it at one.
    pub fn qualify(&mut self, today: NaiveDate) -> bool {
        if self.last_qualified_day == Some(today) {
            return false;
        }
        self.days = match self.last_qualified_day {
            Some(day) if day == today - Duration::days(1) => self.days.saturating_add(1),
            _ => 1,
        };
        self.last_qualified_day = Some(today);
        true
    }
}

/// Represents the user's response to a flashcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewResult {
    /// The user forgot the card (Quality 0-2).
    Again,
    /// The user found the card difficult (Quality 3).
    Hard,
    /// The user remembered the card well (Quality 4).
    Good,
    /// The user remembered the card easily (Quality 5).
    Easy,
}

impl ReviewResult {
    /// Converts the enum to a numeric quality value used by the SM-2 algorithm.
    pub fn to_quality(&self) -> u8 {
        match self {
            ReviewResult::Again => 0,
            ReviewResult::Hard => 3,
            ReviewResult::Good => 4,
            ReviewResult::Easy => 5,
        }
    }
}

/// Stores the state of a flashcard for the scheduling algorithm.
///
/// `interval` is stored in minutes so learning/relearning steps can be scheduled
/// multiple times per day (Anki-like). `next_review` is the source of truth for
/// when the card is due.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CardState {
    /// The number of consecutive successful reviews in review mode.
    pub repetitions: u32,
    /// The current ease factor (EF). Minimum is 1.3.
    pub ease_factor: f32,
    /// The current interval in minutes.
    pub interval: u32,
    /// The date/time when the card is next due for review.
    pub next_review: DateTime<Utc>,
}

impl Default for CardState {
    fn default() -> Self {
        Self {
            repetitions: 0,
            ease_factor: 2.5,
            interval: 0,
            next_review: Utc::now(),
        }
    }
}

/// A flashcard containing content and its current SRS state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Card {
    pub id: i64,
    pub deck: String,
    pub front: String,
    pub back: String,
    /// Optional reading (furigana/kana) shown with the answer.
    pub reading: String,
    /// Optional note or example sentence shown with the answer.
    pub notes: String,
    /// First local day on which the card was introduced (None = new card).
    pub introduced_on: Option<NaiveDate>,
    pub state: CardState,
}

/// A deck of flashcards.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Deck {
    pub id: i64,
    pub name: String,
}

/// Calculates the next state of a card using aggressive, Anki-like
/// learning/relearning steps so cards can be studied multiple times per day.
///
/// `interval` is stored in minutes.
pub fn calculate_next_review(state: &mut CardState, result: ReviewResult) {
    let q = result.to_quality();
    let now = Utc::now();

    // Learning / relearning (not yet graduated to multi-day review intervals).
    let in_learning = state.repetitions == 0 || state.interval < 1440;

    if q < 3 {
        // Again: repeat very soon and count as a lapse in review mode.
        if !in_learning {
            state.ease_factor = (state.ease_factor - 0.2).max(1.3);
        }
        state.repetitions = 0;
        state.interval = 1; // 1 minute
        state.next_review = now + Duration::minutes(1);
        return;
    }

    if in_learning {
        match result {
            ReviewResult::Hard => {
                state.repetitions = 0;
                state.interval = 5; // 5 minutes
                state.next_review = now + Duration::minutes(5);
            }
            ReviewResult::Good => {
                state.repetitions += 1;
                if state.repetitions <= 1 {
                    state.interval = 10; // 10 minutes
                    state.next_review = now + Duration::minutes(10);
                } else {
                    // Graduate: first review interval is 1 day.
                    state.repetitions = 1;
                    state.interval = 1440;
                    state.next_review = now + Duration::days(1);
                }
            }
            ReviewResult::Easy => {
                // Graduate quickly.
                state.repetitions = 1;
                state.interval = 4 * 1440; // 4 days
                state.next_review = now + Duration::days(4);
            }
            ReviewResult::Again => unreachable!(),
        }
        // Small ease nudge for learning answers (kept gentle).
        let adjustment = 0.1 - (q as f32 - 3.0) * 0.05;
        state.ease_factor = (state.ease_factor + adjustment).max(1.3);
        return;
    }

    // Review mode (graduated): Anki-like growth with aggressive same-day recovery
    // handled above via lapses.
    state.repetitions += 1;
    let mut days = (state.interval as f32 / 1440.0).max(1.0);
    days = match result {
        ReviewResult::Hard => (days * 1.2).max(days + 1.0),
        ReviewResult::Good => (days * state.ease_factor).max(days + 1.0),
        ReviewResult::Easy => (days * state.ease_factor * 1.3).max(days + 2.0),
        ReviewResult::Again => unreachable!(),
    };
    let mut interval_days = days.round() as u32;
    if interval_days > 365 {
        interval_days = 365;
    }
    state.interval = interval_days.saturating_mul(1440);

    let adjustment = match result {
        ReviewResult::Hard => -0.15,
        ReviewResult::Good => 0.0,
        ReviewResult::Easy => 0.15,
        ReviewResult::Again => unreachable!(),
    };
    state.ease_factor = (state.ease_factor + adjustment).max(1.3);
    state.next_review = now + Duration::minutes(state.interval as i64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let state = CardState::default();
        assert_eq!(state.repetitions, 0);
        assert_eq!(state.ease_factor, 2.5);
        assert_eq!(state.interval, 0);
    }

    #[test]
    fn test_again_in_learning_is_fast() {
        let mut state = CardState {
            repetitions: 1,
            ease_factor: 2.5,
            interval: 10, // minutes, still in learning
            next_review: Utc::now(),
        };
        calculate_next_review(&mut state, ReviewResult::Again);
        assert_eq!(state.repetitions, 0);
        assert_eq!(state.interval, 1);
    }

    #[test]
    fn test_again_in_review_is_lapse() {
        let mut state = CardState {
            repetitions: 3,
            ease_factor: 2.5,
            interval: 3 * 1440, // graduated review card
            next_review: Utc::now(),
        };
        calculate_next_review(&mut state, ReviewResult::Again);
        assert_eq!(state.repetitions, 0);
        assert!(state.ease_factor < 2.5);
        assert_eq!(state.interval, 1);
    }

    #[test]
    fn test_learning_good_graduates() {
        let mut state = CardState::default();
        calculate_next_review(&mut state, ReviewResult::Good); // first step
        assert_eq!(state.interval, 10);
        calculate_next_review(&mut state, ReviewResult::Good); // graduate
        assert!(state.interval >= 1440);
        assert!(state.next_review > Utc::now());
    }

    #[test]
    fn test_streak_extends_on_consecutive_days() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let mut streak = Streak::default();

        assert_eq!(streak.display_days(day), 0);
        assert!(streak.qualify(day));
        assert_eq!(streak.days, 1);

        assert!(!streak.qualify(day));
        assert_eq!(streak.days, 1);
    }

    #[test]
    fn test_streak_resets_after_gap() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let mut streak = Streak {
            days: 7,
            last_qualified_day: Some(day),
        };

        assert!(streak.qualify(day + Duration::days(2)));
        assert_eq!(streak.days, 1);
        assert_eq!(streak.last_qualified_day, Some(day + Duration::days(2)));
    }
}

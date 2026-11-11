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

/// Stores the state of a flashcard for the SM-2 algorithm.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CardState {
    /// The number of consecutive successful reviews.
    pub repetitions: u32,
    /// The current ease factor (EF). Minimum is 1.3.
    pub ease_factor: f32,
    /// The current interval (in days) between reviews.
    pub interval: u32,
    /// The date when the card is next due for review.
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

/// Calculates the next state of a card based on the user's review result using a simplified and more gradual growth algorithm.
pub fn calculate_next_review(state: &mut CardState, result: ReviewResult) {
    let q = result.to_quality();
    let now = Utc::now();

    if q < 3 {
        // Failure: reset repetitions and decrease ease factor
        state.repetitions = 0;
        state.ease_factor = (state.ease_factor - 0.2).max(1.3);
        state.interval = 1;
        state.next_review = now + Duration::days(1);
    } else {
        // Success
        state.repetitions += 1;

        if state.repetitions == 1 {
            state.interval = 1;
        } else if state.repetitions == 2 {
            state.interval = 2;
        } else if state.repetitions == 3 {
            state.interval = 4;
        } else if state.repetitions == 4 {
            state.interval = 7;
        } else {
            // Gradual growth: interval increases by the ease factor
            state.interval = (state.interval as f32 * state.ease_factor) as u32;
            if state.interval > 180 {
                state.interval = 365;
            }
        }

        // Adjust ease factor: EF' = EF + (0.1 - (5 - q) * (0.08 + (5 - q) * 0.01))
        let adjustment = 0.1 - (q as f32 - 3.0) * 0.1;
        state.ease_factor = (state.ease_factor + adjustment).max(1.3);

        state.next_review = now + Duration::days(state.interval as i64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let state = CardState::default();
        assert_eq!(state.repetitions, 0);
        assert_eq!(state.ease_factor, 2.5);
    }

    #[test]
    fn test_failure_resets_repetitions() {
        let mut state = CardState {
            repetitions: 5,
            ease_factor: 2.5,
            interval: 10,
            next_review: Utc::now(),
        };
        calculate_next_review(&mut state, ReviewResult::Again);
        assert_eq!(state.repetitions, 0);
        assert!(state.ease_factor < 2.5);
        assert_eq!(state.interval, 1);
    }

    #[test]
    fn test_success_increases_interval() {
        let mut state = CardState {
            repetitions: 1,
            ease_factor: 2.5,
            interval: 1,
            next_review: Utc::now(),
        };
        // Second successful review (repetitions goes 1 -> 2)
        calculate_next_review(&mut state, ReviewResult::Good);
        assert_eq!(state.repetitions, 2);
        assert_eq!(state.interval, 2);
    }

    #[test]
    fn test_streak_extends_on_consecutive_days() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let mut streak = Streak::default();

        assert_eq!(streak.display_days(day), 0);
        assert!(streak.qualify(day));
        assert_eq!(streak.days, 1);

        // Qualifying twice on the same day must not double-count.
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

use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};

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
    pub front: String,
    pub back: String,
    pub state: CardState,
}

/// Calculates the next state of a card based on the user's review result using the SM-2 algorithm.
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
            state.interval = 6;
        } else {
            state.interval = (state.interval as f32 * state.ease_factor) as u32;
        }

        // Adjust ease factor: EF' = EF + (0.1 - (5 - q) * (0.08 + (5 - q) * 0.01))
        let adjustment = 0.1 - (5 - q) as f32 * (0.08 + (5 - q) as f32 * 0.01);
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
        assert_eq!(state.interval, 6);
    }
}

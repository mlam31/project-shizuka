use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use crate::core::{Card, CardState};

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Initializes a new database connection and creates necessary tables.
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)
            .context("Failed to open database connection")?;

        let db = Self { conn };
        db.create_tables()?;
        Ok(db)
    }

    /// Creates the required tables for the application.
    fn create_tables(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS cards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                front TEXT NOT NULL,
                back TEXT NOT NULL,
                repetitions INTEGER NOT NULL,
                ease_factor REAL NOT NULL,
                interval INTEGER NOT NULL,
                next_review INTEGER NOT NULL
            )",
            [],
        ).context("Failed to create cards table")?;
        Ok(())
    }

    /// Creates a new flashcard.
    pub fn save_card(&self, front: &str, back: &str) -> Result<i64> {
        let default_state = CardState::default();

        self.conn.execute(
            "INSERT INTO cards (front, back, repetitions, ease_factor, interval, next_review)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                front,
                back,
                default_state.repetitions,
                default_state.ease_factor,
                default_state.interval,
                default_state.next_review.timestamp()
            ],
        ).context("Failed to insert new card")?;

        let id = self.conn.last_insert_rowid();
        Ok(id)
    }

    /// Retrieves all cards that are due for review.
    pub fn get_due_cards(&self) -> Result<Vec<Card>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, front, back, repetitions, ease_factor, interval, next_review
             FROM cards
             WHERE next_review <= ?1
             ORDER BY next_review ASC",
        ).context("Failed to prepare due cards query")?;

        let card_iter = stmt.query_map(params![Utc::now().timestamp()], |row| {
            let next_review_ts = row.get::<_, i64>(6)?;
            Ok(Card {
                id: row.get(0)?,
                front: row.get(1)?,
                back: row.get(2)?,
                state: CardState {
                    repetitions: row.get(3)?,
                    ease_factor: row.get(4)?,
                    interval: row.get(5)?,
                    next_review: DateTime::from_timestamp(next_review_ts, 0)
                        .unwrap_or_else(|| Utc::now()),
                },
            })
        }).context("Failed to iterate over cards")?;

        let mut cards = Vec::new();
        for card in card_iter {
            cards.push(card?);
        }
        Ok(cards)
    }

    /// Updates the state of an existing card.
    pub fn update_card(&self, id: i64, state: &CardState) -> Result<()> {
        self.conn.execute(
            "UPDATE cards
             SET repetitions = ?1, ease_factor = ?2, interval = ?3, next_review = ?4
             WHERE id = ?5",
            params![
                state.repetitions,
                state.ease_factor,
                state.interval,
                state.next_review.timestamp(),
                id,
            ],
        ).context("Failed to update card state")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const TEST_DB: &str = "test_shizuka.db";

    fn cleanup() {
        let _ = fs::remove_file(TEST_DB);
    }

    #[test]
    fn test_db_lifecycle() {
        cleanup();
        let db = Database::new(TEST_DB).unwrap();

        // Test save
        let id = db.save_card("こんにちは", "Hello").unwrap();
        assert_eq!(id, 1);

        // Test retrieval
        let cards = db.get_due_cards().unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].front, "こんにちは");

        // Test update
        let mut state = cards[0].state.clone();
        state.repetitions = 10;
        db.update_card(id, &state).unwrap();

        let updated_cards = db.get_due_cards().unwrap();
        assert_eq!(updated_cards[0].state.repetitions, 10);

        cleanup();
    }
}

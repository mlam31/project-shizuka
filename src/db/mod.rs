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
                deck TEXT NOT NULL,
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

    /// Creates a new flashcard in a specific deck.
    pub fn save_card(&self, deck: &str, front: &str, back: &str) -> Result<i64> {
        let default_state = CardState::default();

        self.conn.execute(
            "INSERT INTO cards (deck, front, back, repetitions, ease_factor, interval, next_review)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                deck,
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

    /// Seeds the database with initial study material.
    pub fn seed_starter_deck(&self) -> Result<()> {
        let deck_name = "Fundamentals";

        // Simple Hiragana/Katakana starter
        let items = vec![
            ("あ", "a"),
            ("い", "i"),
            ("う", "u"),
            ("え", "e"),
            ("お", "o"),
            ("か", "ka"),
            ("き", "ki"),
            ("く", "ku"),
            ("け", "ke"),
            ("こ", "ko"),
        ];

        for (front, back) in items {
            self.save_card(deck_name, front, back)?;
        }

        Ok(())
    }

    /// Retrieves all cards that are due for review in a specific deck.
    pub fn get_due_cards(&self, deck: &str) -> Result<Vec<Card>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, deck, front, back, repetitions, ease_factor, interval, next_review
             FROM cards
             WHERE deck = ?1 AND next_review <= ?2
             ORDER BY next_review ASC",
        ).context("Failed to prepare due cards query")?;

        let card_iter = stmt.query_map(params![deck, Utc::now().timestamp()], |row| {
            let next_review_ts = row.get::<_, i64>(7)?;
            Ok(Card {
                id: row.get(0)?,
                deck: row.get(1)?,
                front: row.get(2)?,
                back: row.get(3)?,
                state: CardState {
                    repetitions: row.get(4)?,
                    ease_factor: row.get(5)?,
                    interval: row.get(6)?,
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

        // Test save with deck
        let id = db.save_card("TestDeck", "こんにちは", "Hello").unwrap();
        assert_eq!(id, 1);

        // Test retrieval
        let cards = db.get_due_cards("TestDeck").unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].front, "こんにちは");

        // Test update
        let mut state = cards[0].state.clone();
        state.repetitions = 10;
        db.update_card(id, &state).unwrap();

        let updated_cards = db.get_due_cards("TestDeck").unwrap();
        assert_eq!(updated_cards[0].state.repetitions, 10);

        cleanup();
    }

    #[test]
    fn test_seeding() {
        cleanup();
        let db = Database::new(TEST_DB).unwrap();
        db.seed_starter_deck().unwrap();

        let cards = db.get_due_cards("Fundamentals").unwrap();
        assert!(cards.len() > 0);

        cleanup();
    }
}

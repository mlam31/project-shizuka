use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
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

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        ).context("Failed to create settings table")?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS deck_meta (
                name TEXT PRIMARY KEY,
                hidden INTEGER NOT NULL DEFAULT 0
            )",
            [],
        ).context("Failed to create deck_meta table")?;

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
        self.seed_alphabet_decks()
    }

    /// Ensures the 6 alphabet decks exist with starter content.
    /// Decks: Hiragana/Katakana/Kanji × Characters/Words.
    pub fn seed_alphabet_decks(&self) -> Result<()> {
        let decks: Vec<(&str, Vec<(&str, &str)>)> = vec![
            (
                "Hiragana Characters",
                vec![
                    ("あ", "a"), ("い", "i"), ("う", "u"), ("え", "e"), ("お", "o"),
                    ("か", "ka"), ("き", "ki"), ("く", "ku"), ("け", "ke"), ("こ", "ko"),
                    ("さ", "sa"), ("し", "shi"), ("す", "su"), ("せ", "se"), ("そ", "so"),
                    ("た", "ta"), ("ち", "chi"), ("つ", "tsu"), ("て", "te"), ("と", "to"),
                    ("な", "na"), ("に", "ni"), ("ぬ", "nu"), ("ね", "ne"), ("の", "no"),
                    ("は", "ha"), ("ひ", "hi"), ("ふ", "fu"), ("へ", "he"), ("ほ", "ho"),
                    ("ま", "ma"), ("み", "mi"), ("む", "mu"), ("め", "me"), ("も", "mo"),
                    ("や", "ya"), ("ゆ", "yu"), ("よ", "yo"),
                    ("ら", "ra"), ("り", "ri"), ("る", "ru"), ("れ", "re"), ("ろ", "ro"),
                    ("わ", "wa"), ("を", "wo"),
                    ("ん", "n"),
                ],
            ),
            (
                "Hiragana Words",
                vec![
                    ("ねこ", "cat"),
                    ("いぬ", "dog"),
                    ("みず", "water"),
                    ("やま", "mountain"),
                    ("さくら", "cherry blossom"),
                ],
            ),
            (
                "Katakana Characters",
                vec![
                    ("ア", "a"),
                    ("イ", "i"),
                    ("ウ", "u"),
                    ("エ", "e"),
                    ("オ", "o"),
                    ("カ", "ka"),
                    ("キ", "ki"),
                    ("ク", "ku"),
                    ("ケ", "ke"),
                    ("コ", "ko"),
                ],
            ),
            (
                "Katakana Words",
                vec![
                    ("コーヒー", "coffee"),
                    ("テレビ", "television"),
                    ("パン", "bread"),
                    ("ホテル", "hotel"),
                    ("タクシー", "taxi"),
                ],
            ),
            (
                "Kanji Characters",
                vec![
                    ("日", "day / sun"),
                    ("月", "month / moon"),
                    ("火", "fire"),
                    ("水", "water"),
                    ("木", "tree"),
                    ("金", "gold / money"),
                    ("土", "earth"),
                    ("人", "person"),
                    ("山", "mountain"),
                    ("川", "river"),
                ],
            ),
            (
                "Kanji Words",
                vec![
                    ("日本", "Japan"),
                    ("学校", "school"),
                    ("先生", "teacher"),
                    ("学生", "student"),
                    ("時間", "time"),
                ],
            ),
        ];

        for (name, items) in decks {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM cards WHERE deck = ?1",
                params![name],
                |row| row.get(0),
            )?;
            if count == 0 {
                for (front, back) in items {
                    self.save_card(name, front, back)?;
                }
            } else {
                // Upgrade partially seeded decks (e.g., Hiragana expanded to 46)
                let mut stmt = self
                    .conn
                    .prepare("SELECT 1 FROM cards WHERE deck = ?1 AND front = ?2 LIMIT 1")
                    .context("Failed to prepare card existence query")?;
                for (front, back) in items {
                    let exists = stmt.exists(params![name, front])?;
                    if !exists {
                        self.save_card(name, front, back)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Lists visible deck names (excludes hidden).
    pub fn list_decks(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT c.deck
                 FROM cards c
                 LEFT JOIN deck_meta m ON m.name = c.deck
                 WHERE COALESCE(m.hidden, 0) = 0
                 ORDER BY c.deck",
            )
            .context("Failed to prepare deck list query")?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .context("Failed to iterate over decks")?;
        let mut decks = Vec::new();
        for row in rows {
            decks.push(row?);
        }
        Ok(decks)
    }

    /// Lists all deck names, including hidden.
    pub fn list_all_decks(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT deck FROM cards ORDER BY deck")
            .context("Failed to prepare deck list query")?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .context("Failed to iterate over decks")?;
        let mut decks = Vec::new();
        for row in rows {
            decks.push(row?);
        }
        Ok(decks)
    }

    pub fn is_deck_hidden(&self, name: &str) -> Result<bool> {
        let hidden: i64 = self
            .conn
            .query_row(
                "SELECT hidden FROM deck_meta WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(hidden != 0)
    }

    pub fn set_deck_hidden(&self, name: &str, hidden: bool) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO deck_meta (name, hidden) VALUES (?1, ?2)
                 ON CONFLICT(name) DO UPDATE SET hidden = excluded.hidden",
                params![name, if hidden { 1i64 } else { 0i64 }],
            )
            .context("Failed to update deck visibility")?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .ok();
        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .context("Failed to save setting")?;
        Ok(())
    }

    /// Exports a deck to CSV (front,back). Returns the path written.
    pub fn export_deck_csv(&self, deck: &str, dir: &Path) -> Result<std::path::PathBuf> {
        fs::create_dir_all(dir).context("Failed to create export directory")?;
        let safe_name: String = deck
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
            .collect();
        let path = dir.join(format!("{}.csv", safe_name.trim()));
        let mut file = File::create(&path).context("Failed to create export file")?;
        writeln!(file, "front,back").context("Failed to write CSV header")?;

        let mut stmt = self
            .conn
            .prepare("SELECT front, back FROM cards WHERE deck = ?1 ORDER BY id ASC")
            .context("Failed to prepare export query")?;
        let rows = stmt.query_map(params![deck], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (front, back) = row?;
            writeln!(file, "{},{}", csv_escape(&front), csv_escape(&back))
                .context("Failed to write CSV row")?;
        }
        Ok(path)
    }

    /// Imports cards from a CSV file (front,back). Returns (deck_name, imported_count).
    pub fn import_deck_csv(&self, path: &Path, deck_name: Option<&str>) -> Result<(String, usize)> {
        let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
        let reader = BufReader::new(file);
        let name = deck_name
            .map(|s| s.to_string())
            .or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "Imported Deck".to_string());

        let mut count = 0usize;
        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if idx == 0 && line.to_ascii_lowercase().starts_with("front") {
                continue;
            }
            let fields = parse_csv_line(line);
            if fields.len() < 2 {
                continue;
            }
            let front = fields[0].trim();
            let back = fields[1].trim();
            if front.is_empty() || back.is_empty() {
                continue;
            }
            let exists = self.conn.query_row(
                "SELECT 1 FROM cards WHERE deck = ?1 AND front = ?2 AND back = ?3 LIMIT 1",
                params![name, front, back],
                |_| Ok(true),
            ).unwrap_or(false);
            if !exists {
                self.save_card(&name, front, back)?;
                count += 1;
            }
        }
        Ok((name, count))
    }

    /// Imports all `.csv` files from a directory.
    pub fn import_decks_from_dir(&self, dir: &Path) -> Result<Vec<(String, usize)>> {
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut results = Vec::new();
        for entry in fs::read_dir(dir).context("Failed to read import directory")? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("csv")).unwrap_or(false) {
                match self.import_deck_csv(&path, None) {
                    Ok(r) => results.push(r),
                    Err(err) => eprintln!("Failed to import {}: {err}", path.display()),
                }
            }
        }
        Ok(results)
    }

    /// Retrieves all cards for a deck (practice/review-ahead; ignores due date).
    pub fn get_all_cards(&self, deck: &str) -> Result<Vec<Card>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, deck, front, back, repetitions, ease_factor, interval, next_review
             FROM cards
             WHERE deck = ?1
             ORDER BY next_review ASC, id ASC",
        ).context("Failed to prepare deck cards query")?;

        let card_iter = stmt.query_map(params![deck], |row| {
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

    /// Retrieves a single card by id.
    pub fn get_card(&self, id: i64) -> Result<Card> {
        self.conn
            .query_row(
                "SELECT id, deck, front, back, repetitions, ease_factor, interval, next_review
                 FROM cards WHERE id = ?1",
                params![id],
                |row| {
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
                },
            )
            .context("Failed to load card")
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

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => {
                fields.push(current.clone());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
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

        let cards = db.get_due_cards("Hiragana Characters").unwrap();
        assert!(!cards.is_empty());

        let words = db.get_due_cards("Kanji Words").unwrap();
        assert!(!words.is_empty());

        cleanup();
    }
}

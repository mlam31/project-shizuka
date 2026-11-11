use crate::core::{Card, CardState, Streak};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Initializes a new database connection and creates necessary tables.
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path).context("Failed to open database connection")?;

        let db = Self { conn };
        db.create_tables()?;
        Ok(db)
    }

    /// Creates the required tables for the application.
    fn create_tables(&self) -> Result<()> {
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS cards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                deck TEXT NOT NULL,
                front TEXT NOT NULL,
                back TEXT NOT NULL,
                reading TEXT NOT NULL DEFAULT '',
                notes TEXT NOT NULL DEFAULT '',
                introduced_on TEXT NOT NULL DEFAULT '',
                repetitions INTEGER NOT NULL,
                ease_factor REAL NOT NULL,
                interval INTEGER NOT NULL,
                next_review INTEGER NOT NULL
            )",
                [],
            )
            .context("Failed to create cards table")?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
                [],
            )
            .context("Failed to create settings table")?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS deck_meta (
                name TEXT PRIMARY KEY,
                hidden INTEGER NOT NULL DEFAULT 0
            )",
                [],
            )
            .context("Failed to create deck_meta table")?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS daily_activity (
                day TEXT PRIMARY KEY,
                seconds INTEGER NOT NULL DEFAULT 0
            )",
                [],
            )
            .context("Failed to create daily_activity table")?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS reviews (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                card_id INTEGER NOT NULL,
                deck TEXT NOT NULL,
                rating INTEGER NOT NULL,
                reviewed_at INTEGER NOT NULL,
                reviewed_on TEXT NOT NULL
            )",
                [],
            )
            .context("Failed to create reviews table")?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_reviews_deck_day
             ON reviews (deck, reviewed_on)",
                [],
            )
            .context("Failed to create review index")?;

        self.migrate_cards()?;
        Ok(())
    }

    /// Adds columns introduced after the first releases and backfills them.
    fn migrate_cards(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(cards)")?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        for (name, definition) in [
            ("reading", "TEXT NOT NULL DEFAULT ''"),
            ("notes", "TEXT NOT NULL DEFAULT ''"),
            ("introduced_on", "TEXT NOT NULL DEFAULT ''"),
        ] {
            if !columns.iter().any(|column| column == name) {
                self.conn
                    .execute(
                        &format!("ALTER TABLE cards ADD COLUMN {} {}", name, definition),
                        [],
                    )
                    .with_context(|| format!("Failed to add cards.{}", name))?;
            }
        }

        // Cards that already have review progress should not be treated as new
        // cards after upgrading.
        self.conn
            .execute(
                "UPDATE cards SET introduced_on = '1970-01-01'
                 WHERE introduced_on = '' AND (repetitions > 0 OR interval > 0)",
                [],
            )
            .context("Failed to backfill introduced_on")?;

        Ok(())
    }

    /// Creates a new flashcard in a specific deck.
    pub fn save_card(&self, deck: &str, front: &str, back: &str) -> Result<i64> {
        self.save_card_full(deck, front, back, "", "")
    }

    /// Creates a new flashcard with an optional reading and notes.
    pub fn save_card_full(
        &self,
        deck: &str,
        front: &str,
        back: &str,
        reading: &str,
        notes: &str,
    ) -> Result<i64> {
        let default_state = CardState::default();

        self.conn
            .execute(
                "INSERT INTO cards
                (deck, front, back, reading, notes, introduced_on,
                 repetitions, ease_factor, interval, next_review)
             VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, ?7, ?8, ?9)",
                params![
                    deck,
                    front,
                    back,
                    reading,
                    notes,
                    default_state.repetitions,
                    default_state.ease_factor,
                    default_state.interval,
                    default_state.next_review.timestamp()
                ],
            )
            .context("Failed to insert new card")?;

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
                    ("さ", "sa"),
                    ("し", "shi"),
                    ("す", "su"),
                    ("せ", "se"),
                    ("そ", "so"),
                    ("た", "ta"),
                    ("ち", "chi"),
                    ("つ", "tsu"),
                    ("て", "te"),
                    ("と", "to"),
                    ("な", "na"),
                    ("に", "ni"),
                    ("ぬ", "nu"),
                    ("ね", "ne"),
                    ("の", "no"),
                    ("は", "ha"),
                    ("ひ", "hi"),
                    ("ふ", "fu"),
                    ("へ", "he"),
                    ("ほ", "ho"),
                    ("ま", "ma"),
                    ("み", "mi"),
                    ("む", "mu"),
                    ("め", "me"),
                    ("も", "mo"),
                    ("や", "ya"),
                    ("ゆ", "yu"),
                    ("よ", "yo"),
                    ("ら", "ra"),
                    ("り", "ri"),
                    ("る", "ru"),
                    ("れ", "re"),
                    ("ろ", "ro"),
                    ("わ", "wa"),
                    ("を", "wo"),
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

        let readings: Vec<(&str, &str)> = vec![
            ("日", "にち / ひ"),
            ("月", "げつ / つき"),
            ("火", "か / ひ"),
            ("水", "すい / みず"),
            ("木", "もく / き"),
            ("金", "きん / かね"),
            ("土", "ど / つち"),
            ("人", "じん / ひと"),
            ("山", "さん / やま"),
            ("川", "せん / かわ"),
            ("日本", "にほん"),
            ("学校", "がっこう"),
            ("先生", "せんせい"),
            ("学生", "がくせい"),
            ("時間", "じかん"),
        ];

        for (name, items) in decks {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM cards WHERE deck = ?1",
                params![name],
                |row| row.get(0),
            )?;
            if count == 0 {
                for (front, back) in items {
                    let reading = reading_for(&readings, front);
                    self.save_card_full(name, front, back, &reading, "")?;
                }
            } else {
                // Upgrade partially seeded decks (e.g., Hiragana expanded to 46)
                let mut stmt = self
                    .conn
                    .prepare("SELECT 1 FROM cards WHERE deck = ?1 AND front = ?2 LIMIT 1")
                    .context("Failed to prepare card existence query")?;
                for (front, back) in items {
                    let exists = stmt.exists(params![name, front])?;
                    let reading = reading_for(&readings, front);
                    if !exists {
                        self.save_card_full(name, front, back, &reading, "")?;
                    } else if !reading.is_empty() {
                        // Enrich existing seeded cards without overwriting edits.
                        self.conn.execute(
                            "UPDATE cards SET reading = ?1
                             WHERE deck = ?2 AND front = ?3 AND reading = ''",
                            params![reading, name, front],
                        )?;
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

    /// Adds exercise seconds to a local calendar day and returns the new total.
    pub fn add_study_seconds(&self, day: &str, seconds: u64) -> Result<u64> {
        if seconds > 0 {
            self.conn
                .execute(
                    "INSERT INTO daily_activity (day, seconds) VALUES (?1, ?2)
                     ON CONFLICT(day) DO UPDATE
                     SET seconds = daily_activity.seconds + excluded.seconds",
                    params![day, seconds as i64],
                )
                .context("Failed to add study time")?;
        }
        self.study_seconds(day)
    }

    /// Returns the exercise seconds recorded for a local calendar day.
    pub fn study_seconds(&self, day: &str) -> Result<u64> {
        let seconds: Option<i64> = self
            .conn
            .query_row(
                "SELECT seconds FROM daily_activity WHERE day = ?1",
                params![day],
                |row| row.get(0),
            )
            .optional()
            .context("Failed to read study time")?;
        Ok(seconds.unwrap_or(0).max(0) as u64)
    }

    /// Loads the persisted streak state.
    pub fn load_streak(&self) -> Result<Streak> {
        let days = self
            .get_setting("streak_days")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let last_qualified_day = self
            .get_setting("streak_last_day")?
            .and_then(|value| NaiveDate::parse_from_str(&value, "%Y-%m-%d").ok());
        Ok(Streak {
            days,
            last_qualified_day,
        })
    }

    /// Persists the streak state.
    pub fn save_streak(&self, streak: &Streak) -> Result<()> {
        self.set_setting("streak_days", &streak.days.to_string())?;
        match streak.last_qualified_day {
            Some(day) => {
                self.set_setting("streak_last_day", &day.format("%Y-%m-%d").to_string())?
            }
            None => {
                self.conn
                    .execute("DELETE FROM settings WHERE key = 'streak_last_day'", [])
                    .context("Failed to clear streak day")?;
            }
        }
        Ok(())
    }

    /// Exports a deck to CSV (front,back,reading,notes). Returns the path written.
    pub fn export_deck_csv(&self, deck: &str, dir: &Path) -> Result<std::path::PathBuf> {
        fs::create_dir_all(dir).context("Failed to create export directory")?;
        let safe_name: String = deck
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = dir.join(format!("{}.csv", safe_name.trim()));
        let mut file = File::create(&path).context("Failed to create export file")?;
        writeln!(file, "front,back,reading,notes").context("Failed to write CSV header")?;

        let mut stmt = self
            .conn
            .prepare(
                "SELECT front, back, reading, notes FROM cards WHERE deck = ?1 ORDER BY id ASC",
            )
            .context("Failed to prepare export query")?;
        let rows = stmt.query_map(params![deck], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (front, back, reading, notes) = row?;
            writeln!(
                file,
                "{},{},{},{}",
                csv_escape(&front),
                csv_escape(&back),
                csv_escape(&reading),
                csv_escape(&notes)
            )
            .context("Failed to write CSV row")?;
        }
        Ok(path)
    }

    /// Imports cards from CSV lines with `front,back[,reading[,notes]]`.
    fn import_csv_records<R: BufRead>(&self, reader: R, name: &str) -> Result<usize> {
        let mut count = 0usize;
        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let fields = parse_csv_line(line);
            if idx == 0
                && fields
                    .first()
                    .map(|field| field.trim().eq_ignore_ascii_case("front"))
                    .unwrap_or(false)
            {
                continue;
            }
            if fields.len() < 2 {
                continue;
            }
            let front = fields[0].trim();
            let back = fields[1].trim();
            let reading = fields.get(2).map(|s| s.trim()).unwrap_or("");
            let notes = fields.get(3).map(|s| s.trim()).unwrap_or("");
            if front.is_empty() || back.is_empty() {
                continue;
            }
            let exists = self
                .conn
                .query_row(
                    "SELECT 1 FROM cards WHERE deck = ?1 AND front = ?2 AND back = ?3 LIMIT 1",
                    params![name, front, back],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if !exists {
                self.save_card_full(name, front, back, reading, notes)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Imports cards from a CSV file. Returns (deck_name, imported_count).
    pub fn import_deck_csv(&self, path: &Path, deck_name: Option<&str>) -> Result<(String, usize)> {
        let file =
            File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
        let name = deck_name
            .map(|s| s.to_string())
            .or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "Imported Deck".to_string());

        let count = self.import_csv_records(BufReader::new(file), &name)?;
        Ok((name, count))
    }

    /// Imports pasted CSV text (no file picker needed on mobile).
    pub fn import_deck_csv_text(&self, text: &str, deck_name: &str) -> Result<(String, usize)> {
        let name = if deck_name.trim().is_empty() {
            "Imported Deck"
        } else {
            deck_name.trim()
        };
        let count = self.import_csv_records(BufReader::new(text.as_bytes()), name)?;
        Ok((name.to_string(), count))
    }

    /// Writes a consistent copy of the database to `path`.
    ///
    /// `VACUUM INTO` works while the app keeps using the live connection, so a
    /// backup never needs to close the database.
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        // VACUUM INTO refuses to overwrite, and replacing an old backup is the
        // expected behaviour when the file name is reused.
        let _ = fs::remove_file(path);
        let escaped = path.to_string_lossy().replace('\'', "''");
        self.conn
            .execute_batch(&format!("VACUUM INTO '{}'", escaped))
            .with_context(|| format!("Failed to back up database to {}", path.display()))?;
        Ok(())
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
            if path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("csv"))
                .unwrap_or(false)
            {
                match self.import_deck_csv(&path, None) {
                    Ok(r) => results.push(r),
                    Err(err) => eprintln!("Failed to import {}: {err}", path.display()),
                }
            }
        }
        Ok(results)
    }

    /// Retrieves all cards for a deck (practice mode; ignores due dates).
    pub fn get_all_cards(&self, deck: &str) -> Result<Vec<Card>> {
        let sql = format!(
            "{} WHERE deck = ?1 ORDER BY next_review ASC, id ASC",
            CARD_SELECT
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("Failed to prepare deck cards query")?;
        let rows = stmt
            .query_map(params![deck], card_from_row)
            .context("Failed to iterate over cards")?;
        let mut cards = Vec::new();
        for row in rows {
            cards.push(row?);
        }
        Ok(cards)
    }

    /// Retrieves cards whose next review is due.
    pub fn get_due_cards(&self, deck: &str) -> Result<Vec<Card>> {
        let sql = format!(
            "{} WHERE deck = ?1 AND introduced_on <> '' AND next_review <= ?2
             ORDER BY next_review ASC",
            CARD_SELECT
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("Failed to prepare due cards query")?;
        let rows = stmt
            .query_map(params![deck, Utc::now().timestamp()], card_from_row)
            .context("Failed to iterate over due cards")?;
        let mut cards = Vec::new();
        for row in rows {
            cards.push(row?);
        }
        Ok(cards)
    }

    /// Retrieves up to `limit` cards that have never been introduced.
    pub fn get_new_cards(&self, deck: &str, limit: usize) -> Result<Vec<Card>> {
        let sql = format!(
            "{} WHERE deck = ?1 AND introduced_on = '' ORDER BY id ASC LIMIT ?2",
            CARD_SELECT
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("Failed to prepare new cards query")?;
        let rows = stmt
            .query_map(params![deck, limit as i64], card_from_row)
            .context("Failed to iterate over new cards")?;
        let mut cards = Vec::new();
        for row in rows {
            cards.push(row?);
        }
        Ok(cards)
    }

    /// Retrieves a single card by id.
    pub fn get_card(&self, id: i64) -> Result<Card> {
        let sql = format!("{} WHERE id = ?1", CARD_SELECT);
        self.conn
            .query_row(&sql, params![id], card_from_row)
            .context("Failed to load card")
    }

    /// Records a rating, updates the SRS state, and introduces the card if needed.
    pub fn record_review(
        &self,
        card: &Card,
        state: &CardState,
        rating: i32,
        day: &str,
    ) -> Result<()> {
        let tx = self
            .conn
            .unchecked_transaction()
            .context("Failed to start review transaction")?;
        if card.introduced_on.is_none() {
            tx.execute(
                "UPDATE cards SET repetitions = ?1, ease_factor = ?2, interval = ?3,
                 next_review = ?4, introduced_on = ?5 WHERE id = ?6",
                params![
                    state.repetitions,
                    state.ease_factor,
                    state.interval,
                    state.next_review.timestamp(),
                    day,
                    card.id
                ],
            )
            .context("Failed to introduce card")?;
        } else {
            tx.execute(
                "UPDATE cards SET repetitions = ?1, ease_factor = ?2, interval = ?3,
                 next_review = ?4 WHERE id = ?5",
                params![
                    state.repetitions,
                    state.ease_factor,
                    state.interval,
                    state.next_review.timestamp(),
                    card.id
                ],
            )
            .context("Failed to update card state")?;
        }
        tx.execute(
            "INSERT INTO reviews (card_id, deck, rating, reviewed_at, reviewed_on)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![card.id, card.deck, rating, Utc::now().timestamp(), day],
        )
        .context("Failed to log review")?;
        tx.commit().context("Failed to commit review")?;
        Ok(())
    }

    /// Number of cards introduced on a local day.
    pub fn count_new_introduced(&self, deck: &str, day: &str) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM cards WHERE deck = ?1 AND introduced_on = ?2",
                params![deck, day],
                |row| row.get(0),
            )
            .context("Failed to count new cards")?;
        Ok(count as usize)
    }

    /// Number of ratings logged on a local day.
    pub fn count_reviews_on(&self, deck: &str, day: &str) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM reviews WHERE deck = ?1 AND reviewed_on = ?2",
                params![deck, day],
                |row| row.get(0),
            )
            .context("Failed to count reviews")?;
        Ok(count as usize)
    }

    /// Returns (new cards never seen, cards introduced today, cards due now).
    pub fn deck_counts(&self, deck: &str, day: &str) -> Result<(usize, usize, usize)> {
        let new_total: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM cards WHERE deck = ?1 AND introduced_on = ''",
            params![deck],
            |row| row.get(0),
        )?;
        let introduced_today: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM cards WHERE deck = ?1 AND introduced_on = ?2",
            params![deck, day],
            |row| row.get(0),
        )?;
        let due: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM cards
             WHERE deck = ?1 AND introduced_on <> '' AND next_review <= ?2",
            params![deck, Utc::now().timestamp()],
            |row| row.get(0),
        )?;
        Ok((new_total as usize, introduced_today as usize, due as usize))
    }

    /// Updates the state of an existing card.
    pub fn update_card(&self, id: i64, state: &CardState) -> Result<()> {
        self.conn
            .execute(
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
            )
            .context("Failed to update card state")?;
        Ok(())
    }
}

/// Columns used by every card query. Keep in sync with [`card_from_row`].
const CARD_SELECT: &str = "SELECT id, deck, front, back, reading, notes, \
     repetitions, ease_factor, interval, next_review, introduced_on FROM cards";

/// Builds a [`Card`] from a row selected with [`CARD_SELECT`].
fn card_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Card> {
    let next_review: i64 = row.get(9)?;
    let introduced_on: String = row.get(10)?;
    Ok(Card {
        id: row.get(0)?,
        deck: row.get(1)?,
        front: row.get(2)?,
        back: row.get(3)?,
        reading: row.get(4)?,
        notes: row.get(5)?,
        state: CardState {
            repetitions: row.get(6)?,
            ease_factor: row.get(7)?,
            interval: row.get(8)?,
            next_review: DateTime::from_timestamp(next_review, 0).unwrap_or_else(Utc::now),
        },
        introduced_on: NaiveDate::parse_from_str(&introduced_on, "%Y-%m-%d").ok(),
    })
}

/// Looks up the reading for a seeded card, if one is known.
fn reading_for(readings: &[(&str, &str)], front: &str) -> String {
    readings
        .iter()
        .find(|(name, _)| *name == front)
        .map(|(_, reading)| (*reading).to_string())
        .unwrap_or_default()
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
    use crate::core::{calculate_next_review, ReviewResult};
    use std::fs;

    const TEST_DB: &str = "test_shizuka.db";

    fn cleanup() {
        let _ = fs::remove_file(TEST_DB);
    }

    #[test]
    fn test_db_lifecycle() {
        cleanup();
        let db = Database::new(TEST_DB).unwrap();

        // A fresh card is new, not due.
        let id = db.save_card("TestDeck", "こんにちは", "Hello").unwrap();
        assert_eq!(id, 1);
        assert!(db.get_due_cards("TestDeck").unwrap().is_empty());

        let new_cards = db.get_new_cards("TestDeck", 10).unwrap();
        assert_eq!(new_cards.len(), 1);
        assert_eq!(new_cards[0].front, "こんにちは");
        assert_eq!(new_cards[0].introduced_on, None);

        // Rating a new card introduces it and schedules it in the future.
        let card = new_cards[0].clone();
        let mut state = card.state.clone();
        calculate_next_review(&mut state, ReviewResult::Good);
        db.record_review(&card, &state, 2, "2026-01-01").unwrap();

        assert_eq!(db.get_new_cards("TestDeck", 10).unwrap().len(), 0);
        assert_eq!(
            db.count_new_introduced("TestDeck", "2026-01-01").unwrap(),
            1
        );
        assert_eq!(db.count_reviews_on("TestDeck", "2026-01-01").unwrap(), 1);
        assert!(db.get_due_cards("TestDeck").unwrap().is_empty());

        // A card that is past its due date shows up again.
        let mut due_state = db.get_card(id).unwrap().state;
        due_state.next_review = Utc::now() - chrono::Duration::days(1);
        db.update_card(id, &due_state).unwrap();
        let due = db.get_due_cards("TestDeck").unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].state.repetitions, 1);

        cleanup();
    }

    #[test]
    fn test_seeding() {
        const SEED_DB: &str = "test_shizuka_seed.db";
        let _ = fs::remove_file(SEED_DB);
        let db = Database::new(SEED_DB).unwrap();
        db.seed_starter_deck().unwrap();

        let cards = db.get_new_cards("Hiragana Characters", 100).unwrap();
        assert_eq!(cards.len(), 46);
        assert!(db.get_due_cards("Hiragana Characters").unwrap().is_empty());

        let words = db.get_new_cards("Kanji Words", 100).unwrap();
        assert_eq!(words.len(), 5);
        assert_eq!(words[0].reading, "にほん");

        let _ = fs::remove_file(SEED_DB);
    }

    #[test]
    fn test_csv_text_import_and_backup() {
        const CSV_DB: &str = "test_shizuka_csv.db";
        const BACKUP_DB: &str = "test_shizuka_backup.db";
        let _ = fs::remove_file(CSV_DB);
        let _ = fs::remove_file(BACKUP_DB);
        let db = Database::new(CSV_DB).unwrap();

        let csv = "front,back,reading,notes\n食べる,to eat,たべる,Example: ご飯を食べる\n";
        let (name, count) = db.import_deck_csv_text(csv, "Food").unwrap();
        assert_eq!(name, "Food");
        assert_eq!(count, 1);
        assert_eq!(db.import_deck_csv_text(csv, "Food").unwrap().1, 0);

        let cards = db.get_new_cards("Food", 10).unwrap();
        assert_eq!(cards[0].reading, "たべる");
        assert_eq!(cards[0].notes, "Example: ご飯を食べる");

        db.backup_to(std::path::Path::new(BACKUP_DB)).unwrap();
        assert!(std::path::Path::new(BACKUP_DB).exists());
        let backup = Database::new(BACKUP_DB).unwrap();
        assert_eq!(backup.get_new_cards("Food", 10).unwrap().len(), 1);

        let _ = fs::remove_file(CSV_DB);
        let _ = fs::remove_file(BACKUP_DB);
    }

    #[test]
    fn test_daily_activity_and_streak_persistence() {
        const STREAK_DB: &str = "test_shizuka_streak.db";
        let _ = fs::remove_file(STREAK_DB);
        let db = Database::new(STREAK_DB).unwrap();

        assert_eq!(db.study_seconds("2026-01-01").unwrap(), 0);
        assert_eq!(db.add_study_seconds("2026-01-01", 300).unwrap(), 300);
        assert_eq!(db.add_study_seconds("2026-01-01", 600).unwrap(), 900);
        assert_eq!(db.study_seconds("2026-01-01").unwrap(), 900);

        let streak = Streak {
            days: 2,
            last_qualified_day: NaiveDate::from_ymd_opt(2026, 1, 1),
        };
        db.save_streak(&streak).unwrap();
        assert_eq!(db.load_streak().unwrap(), streak);

        let _ = fs::remove_file(STREAK_DB);
    }
}

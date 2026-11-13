// Hide the console window on Windows release builds.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod core;
mod db;
mod ui;

use anyhow::{Context, Result};
use chrono::Local;
use rand::Rng;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::path::PathBuf;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;
use std::rc::Rc;
use std::time::Instant;

use crate::core::{
    calculate_next_review, Card, CardState, ReviewResult, Streak, DAILY_GOAL_SECONDS,
};
use crate::db::Database;
use crate::ui::{AppState, AppWindow, DeckInfo, ManageDeck};

/// Longest stretch counted from a single card, so an idle app cannot farm
/// study time (and a streak) by simply being left open.
const MAX_CARD_SECONDS: u64 = 120;

/// Number of new cards introduced per deck and day unless the user changes it.
const DEFAULT_NEW_CARD_LIMIT: usize = 10;

/// Everything needed while a review session is running.
#[derive(Default)]
struct Session {
    /// Cards still waiting to be shown. Cards rated Bad are pushed back here.
    queue: VecDeque<Card>,
    deck: String,
    reviewed: u32,
    correct: u32,
    total: u32,
    /// Cards missed during this session, with the number of misses.
    missed: HashMap<i64, (String, u32)>,
    /// How many cards were rated 'Easy' (Very good) in this session.
    easy: u32,
}

impl Session {
    /// Applies a rating to the session counters and queue.
    ///
    /// Cards rated Bad go back to the end of the queue so they are seen again;
    /// every other rating retires the card for this session, which is what
    /// stops "Very good" from showing the same card over and over.
    fn record(&mut self, card: &Card, rating: i32) {
        self.reviewed += 1;
        if rating == 0 {
            let entry = self
                .missed
                .entry(card.id)
                .or_insert_with(|| (card.front.clone(), 0));
            entry.1 += 1;
            self.queue.push_back(card.clone());
        } else {
            self.correct += 1;
            if rating == 3 {
                self.easy += 1;
            }
        }
    }
}

fn rating_to_result(rating: i32) -> ReviewResult {
    match rating {
        0 => ReviewResult::Again, // Bad
        1 => ReviewResult::Hard,  // OK
        2 => ReviewResult::Good,  // Good
        _ => ReviewResult::Easy,  // Very good
    }
}

/// Locally stored daily new-card limit.
fn new_card_limit(db: &Database) -> usize {
    db.get_setting("new_card_limit")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_NEW_CARD_LIMIT)
        .clamp(1, 500)
}

fn today_key() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// Refreshes the deck list with due/new counts and the manage list.
fn refresh_deck_lists(ui: &AppWindow, db: &Database) {
    let visible = db.list_decks().unwrap_or_default();
    let all = db.list_all_decks().unwrap_or_default();
    let day = today_key();
    let base_limit = new_card_limit(db);

    let infos: Vec<DeckInfo> = visible
        .into_iter()
        .map(|name| {
            let (new_total, introduced_today, due) =
                db.deck_counts(&name, &day).unwrap_or((0, 0, 0));
            let new_available = new_total.min(base_limit.saturating_sub(introduced_today));
            let counts_text = if new_total == 0 {
                format!("{due} due")
            } else if introduced_today >= base_limit {
                format!("{due} due · {new_total} more new (limit reached)")
            } else {
                format!("{due} due · {new_available} new today")
            };
            DeckInfo {
                name: name.into(),
                counts_text: counts_text.into(),
            }
        })
        .collect();
    ui.set_deck_infos(ModelRc::new(VecModel::from(infos)));

    let manage: Vec<ManageDeck> = all
        .into_iter()
        .map(|name| {
            let hidden = db.is_deck_hidden(&name).unwrap_or(false);
            ManageDeck {
                name: name.into(),
                selected: false,
                hidden,
            }
        })
        .collect();
    ui.set_manage_decks(ModelRc::new(VecModel::from(manage)));
}

/// Updates the stored streak for the current local day.
///
/// Returns the streak and today's exercise seconds.
fn sync_streak(db: &Database) -> (Streak, u64) {
    let today = Local::now().date_naive();
    let day_key = today.format("%Y-%m-%d").to_string();
    let today_seconds = db.study_seconds(&day_key).unwrap_or(0);

    let mut streak = db.load_streak().unwrap_or_default();
    if today_seconds >= DAILY_GOAL_SECONDS && streak.qualify(today) {
        if let Err(err) = db.save_streak(&streak) {
            eprintln!("Failed to save streak: {err}");
        }
    }
    (streak, today_seconds)
}

/// Refreshes the home-menu streak and today's exercise progress.
///
/// Qualifying days are stored once the 15-minute goal is reached, so the
/// streak advances even if the app is closed and reopened later that day.
fn refresh_streak(ui: &AppWindow, db: &Database) {
    let today = Local::now().date_naive();
    let (streak, today_seconds) = sync_streak(db);

    let days = streak.display_days(today);
    let streak_text = match days {
        0 => "No streak yet".to_string(),
        1 => "1-day streak".to_string(),
        n => format!("{n}-day streak"),
    };
    let minutes = today_seconds / 60;
    let today_text = if today_seconds >= DAILY_GOAL_SECONDS {
        format!("{minutes} min today - goal met")
    } else {
        format!("{minutes} / {} min today", DAILY_GOAL_SECONDS / 60)
    };
    let progress = if DAILY_GOAL_SECONDS > 0 {
        (today_seconds as f32 / DAILY_GOAL_SECONDS as f32).min(1.0)
    } else {
        0.0
    };
    let goal_text = if today_seconds >= DAILY_GOAL_SECONDS {
        "15 min daily goal - achieved".to_string()
    } else {
        format!("{} / {} min today", minutes, DAILY_GOAL_SECONDS / 60)
    };

    ui.set_streak_text(streak_text.into());
    ui.set_today_text(today_text.into());
    ui.set_today_progress(progress);
    ui.set_today_goal_text(goal_text.into());
}

/// Credits the time spent on the card currently on screen, then clears it.
///
/// Called when a card is rated, when the user leaves the deck, and on exit.
fn record_study_time(db: &Database, shown_at: &Cell<Option<Instant>>) {
    let studied = shown_at
        .take()
        .map(|shown| shown.elapsed().as_secs().min(MAX_CARD_SECONDS))
        .unwrap_or(0);
    if studied == 0 {
        return;
    }
    let day = today_key();
    if let Err(err) = db.add_study_seconds(&day, studied) {
        eprintln!("Failed to record study time: {err}");
    }
}

fn format_interval(minutes: u32) -> String {
    if minutes == 0 {
        return "<1m".to_string();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    if minutes < 1440 {
        let hours = ((minutes as f32) / 60.0).round() as u32;
        return format!("{}h", hours.max(1));
    }
    let days = ((minutes as f32) / 1440.0).round() as u32;
    match days {
        0 => "1d".to_string(),
        1 => "1d".to_string(),
        d if d < 30 => format!("{d}d"),
        d => format!("{}mo", ((d as f32 / 30.0).round() as u32).max(1)),
    }
}

/// Next interval shown on each rating button, in rating order.
fn interval_previews(state: &CardState) -> [String; 4] {
    [
        ReviewResult::Again,
        ReviewResult::Hard,
        ReviewResult::Good,
        ReviewResult::Easy,
    ]
    .map(|result| {
        let mut next = state.clone();
        calculate_next_review(&mut next, result);
        format_interval(next.interval)
    })
}

fn contains_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c as u32,
            0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xFF66..=0xFF9F
        )
    })
}

/// Picks the side of the card that should be spoken aloud.
fn speak_text(card: &Card) -> String {
    if contains_japanese(&card.front) {
        card.front.clone()
    } else if contains_japanese(&card.back) {
        card.back.clone()
    } else {
        card.front.clone()
    }
}

/// Builds a queue and starts showing cards.
///
/// `practice` ignores due dates and the daily new-card limit, which powers the
/// "Keep learning" button after a session ends.
fn start_session(
    ui: &AppWindow,
    db: &Database,
    deck: &str,
    practice: bool,
    session: &RefCell<Session>,
    shown_at: &Cell<Option<Instant>>,
) {
    let day = today_key();
    let cards = if practice {
        // Focused practice: prioritize cards that are struggling or due soon
        // instead of a random scroll through the whole deck.
        let mut all = db.get_all_cards(deck).unwrap_or_default();
        all.sort_by(|a, b| {
            let a_learn = a.state.interval < 1440 || a.state.repetitions == 0;
            let b_learn = b.state.interval < 1440 || b.state.repetitions == 0;
            b_learn
                .cmp(&a_learn)
                .then(a.state.next_review.cmp(&b.state.next_review))
                .then(
                    a.state
                        .ease_factor
                        .partial_cmp(&b.state.ease_factor)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then(a.state.repetitions.cmp(&b.state.repetitions))
        });
        all.truncate(40);
        all
    } else {
        let mut due = db.get_due_cards(deck).unwrap_or_default();
        let base_limit = new_card_limit(db);
        let introduced_today = db.count_new_introduced(deck, &day).unwrap_or(0);
        let remaining = base_limit.saturating_sub(introduced_today);
        due.extend(db.get_new_cards(deck, remaining).unwrap_or_default());
        due
    };

    {
        let mut state = session.borrow_mut();
        let total = cards.len() as u32;
        *state = Session {
            queue: cards.into_iter().collect(),
            deck: deck.to_string(),
            total,
            ..Session::default()
        };
    }

    if session.borrow().queue.is_empty() {
        show_summary(ui, db, session);
    } else {
        show_next_card(ui, db, session, shown_at);
    }
}

/// Continues with the next ready cards (including learning steps due soon)
/// instead of a random full-deck scroll.
fn auto_continue_session(
    ui: &AppWindow,
    db: &Database,
    session: &RefCell<Session>,
    shown_at: &Cell<Option<Instant>>,
) {
    let deck = session.borrow().deck.clone();
    // Prefer due/ready cards; if none, fall back to focused practice.
    start_session(ui, db, &deck, false, session, shown_at);
}

/// Shows the next queued card, or the summary when the queue is empty.
fn show_next_card(
    ui: &AppWindow,
    db: &Database,
    session: &RefCell<Session>,
    shown_at: &Cell<Option<Instant>>,
) {
    let card = session.borrow_mut().queue.pop_front();
    let Some(card) = card else {
        shown_at.set(None);
        show_summary(ui, db, session);
        return;
    };

    let mut rng = rand::thread_rng();
    let reverse = match ui.get_card_direction().to_string().as_str() {
        "character" => false,
        "pronunciation" => true,
        _ => rng.gen_bool(0.5),
    };
    let (prompt, answer) = if reverse {
        (card.back.clone(), card.front.clone())
    } else {
        (card.front.clone(), card.back.clone())
    };

    let intervals = interval_previews(&card.state);
    let left = session.borrow().queue.len() + 1;

    ui.set_selected_deck(card.deck.clone().into());
    ui.set_current_card_id(card.id as i32);
    ui.set_card_front(prompt.into());
    ui.set_card_back(answer.into());
    ui.set_card_reading(card.reading.clone().into());
    ui.set_card_notes(card.notes.clone().into());
    ui.set_card_speak_text(speak_text(&card).into());
    ui.set_interval_bad(intervals[0].clone().into());
    ui.set_interval_ok(intervals[1].clone().into());
    ui.set_interval_good(intervals[2].clone().into());
    ui.set_interval_easy(intervals[3].clone().into());
    ui.set_session_progress(format!("{left} left").into());
    ui.set_is_revealed(false);
    ui.set_current_state(AppState::Reviewing);
    shown_at.set(Some(Instant::now()));
}

/// Ends the session and shows reviewed/missed cards.
fn show_summary(ui: &AppWindow, db: &Database, session: &RefCell<Session>) {
    let (title, stats, recap, reviewed, easy) = {
        let state = session.borrow();
        let reviewed = state.reviewed;
        let correct = state.correct;
        let easy = state.easy;
        let accuracy = correct
            .saturating_mul(100)
            .checked_div(reviewed)
            .unwrap_or(0);
        let minutes = db.study_seconds(&today_key()).unwrap_or(0) / 60;

        let (title, stats) = if reviewed == 0 {
            (
                "Deck finished",
                "Keep learning to practice cards ahead of schedule.".to_string(),
            )
        } else {
            (
                "Session complete",
                format!("{reviewed} reviewed · {accuracy}% remembered · {minutes} min today"),
            )
        };

        let mut missed: Vec<(String, u32)> = state.missed.values().cloned().collect();
        missed.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let recap: Vec<slint::SharedString> = missed
            .into_iter()
            .map(|(word, count)| {
                if count > 1 {
                    format!("{word} — missed {count}×").into()
                } else {
                    format!("{word} — missed once").into()
                }
            })
            .collect();

        (title, stats, recap, reviewed, easy)
    };

    ui.set_summary_title(title.into());
    ui.set_summary_stats(stats.into());
    // Missed cards get the "fall and rise" motto; a clean session gets the
    // progress one.
    let motto = if recap.is_empty() {
        &MOTTO_DAILY_PROGRESS
    } else {
        &MOTTO_FALL_AND_RISE
    };
    ui.set_summary_motto_kanji(motto.kanji.into());
    ui.set_summary_motto_romaji(motto.romaji.into());
    ui.set_summary_motto_meaning(motto.meaning.into());
    ui.set_recap_words(ModelRc::new(VecModel::from(recap)));
    ui.set_session_progress("".into());
    ui.set_current_state(AppState::Summary);

    if reviewed > 0 && easy == reviewed {
        let current_limit = new_card_limit(db);
        let new_limit = current_limit + 1;
        if let Err(err) = db.set_setting("new_card_limit", &new_limit.to_string()) {
            eprintln!("Failed to increment limit: {err}");
        } else {
            ui.set_status_message(format!("Perfect session! New daily limit: {new_limit}").into());
        }
    }
}

/// Directory where exported decks and backups are written.
fn export_dir() -> PathBuf {
    #[cfg(target_os = "ios")]
    {
        if let Some(home) = env::var_os("HOME") {
            // The app sandbox has no ~/Downloads; Documents is visible in the Files app.
            return PathBuf::from(home).join("Documents").join("Shizuka");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(profile) = env::var_os("USERPROFILE") {
            return PathBuf::from(profile).join("Downloads").join("Shizuka");
        }
    }
    #[cfg(not(any(target_os = "ios", target_os = "windows")))]
    {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join("Downloads").join("Shizuka");
        }
    }
    PathBuf::from("exports")
}

/// Default location of the SQLite database.
///
/// On iOS the bundle is read-only, so the database lives in the app's Documents
/// directory (writable, and accessible through the Files app). On Windows it
/// lives in the user's Documents folder so it survives updates of the exe.
fn default_db_path() -> PathBuf {
    #[cfg(target_os = "ios")]
    {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join("Documents").join("shizuka.db");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(profile) = env::var_os("USERPROFILE") {
            return PathBuf::from(profile)
                .join("Documents")
                .join("Shizuka")
                .join("shizuka.db");
        }
        if let Some(appdata) = env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Shizuka").join("shizuka.db");
        }
    }
    #[cfg(target_os = "macos")]
    {
        // Keep using a project-local database while developing, but use
        // Application Support for the packaged .app (whose cwd is `/`).
        let local = PathBuf::from("shizuka.db");
        if local.exists() {
            return local;
        }
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Shizuka")
                .join("shizuka.db");
        }
    }
    PathBuf::from("shizuka.db")
}

/// Japanese-capable UI font preference.
///
/// Prefer fonts known to include kana/kanji on the target platform. Slint will
/// fall back to other system fonts if the named family is unavailable, but
/// providing a sensible default improves the chance katakana render correctly.
fn ui_font_family() -> &'static str {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        "Hiragino Sans"
    }
    #[cfg(target_os = "windows")]
    {
        "Yu Gothic UI"
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "windows")))]
    {
        "Noto Sans CJK JP"
    }
}

/// A yojijukugo shown to the learner: kanji, romaji, and English meaning.
struct Motto {
    kanji: &'static str,
    romaji: &'static str,
    meaning: &'static str,
}

/// App-wide motto: the SRS promise behind spaced repetition — a card you miss
/// is a fall, not a defeat; the scheduler keeps bringing it back until it
/// sticks, so you "stand up on the eighth try".
const MOTTO_FALL_AND_RISE: Motto = Motto {
    kanji: "七転八起き",
    romaji: "nanakorobi yaoki",
    meaning: "Fall seven times, stand up eight times",
};

/// Shown after a clean session: keep the daily streak going.
const MOTTO_DAILY_PROGRESS: Motto = Motto {
    kanji: "日進月歩",
    romaji: "nisshin gepo",
    meaning: "A little progress every day",
};

fn import_dir(db_path: &str) -> PathBuf {
    let p = PathBuf::from(db_path);
    p.parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("import")
}

fn selected_manage_decks(ui: &AppWindow) -> Vec<String> {
    let model = ui.get_manage_decks();
    let mut out = Vec::new();
    for i in 0..model.row_count() {
        if let Some(row) = model.row_data(i) {
            if row.selected {
                out.push(row.name.to_string());
            }
        }
    }
    out
}

/// Reads text from the system clipboard without an extra dependency.
#[cfg(target_os = "ios")]
fn read_clipboard() -> Option<String> {
    use objc2_ui_kit::UIPasteboard;
    unsafe {
        let board = UIPasteboard::generalPasteboard();
        board.string().map(|text| text.to_string())
    }
}

#[cfg(target_os = "macos")]
fn read_clipboard() -> Option<String> {
    let output = Command::new("pbpaste").output().ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn read_clipboard() -> Option<String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-Clipboard -Raw",
        ])
        .output()
        .ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

#[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "windows")))]
fn read_clipboard() -> Option<String> {
    None
}

/// Per-user writable directory, used when the current directory isn't writable
/// (for example a .app launched from Finder, whose working directory is `/`).
fn user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env::var_os("HOME") {
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("Shizuka"),
            );
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env::var_os("APPDATA") {
            return Some(PathBuf::from(appdata).join("Shizuka"));
        }
    }
    None
}

/// Opens the database, falling back to the per-user data directory when the
/// requested path isn't writable.
fn open_database(path: &str) -> Result<(Database, String)> {
    match Database::new(path) {
        Ok(db) => Ok((db, path.to_string())),
        Err(err) if env::var_os("SHIZUKA_DB").is_none() => {
            let Some(dir) = user_data_dir() else {
                return Err(err);
            };
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create {}", dir.display()))?;
            let fallback = dir.join("shizuka.db").to_string_lossy().into_owned();
            eprintln!("Could not open {path} ({err:#}); using {fallback}");
            Ok((Database::new(&fallback)?, fallback))
        }
        Err(err) => Err(err),
    }
}

fn main() -> Result<()> {
    let db_path = env::var("SHIZUKA_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_db_path())
        .to_string_lossy()
        .into_owned();
    if let Some(parent) = PathBuf::from(&db_path).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let (db, db_path) = open_database(&db_path)?;
    println!("Using database: {}", db_path);
    db.seed_alphabet_decks()?;
    // Create the import folder up front so it shows up in the Files app on iOS
    // (and is ready for imports on desktop).
    let import_path = import_dir(&db_path);
    if let Err(err) = std::fs::create_dir_all(&import_path) {
        eprintln!(
            "Failed to create import folder {}: {err}",
            import_path.display()
        );
    }

    let ui = AppWindow::new()?;
    ui.set_ui_font_family(ui_font_family().into());
    ui.set_motto_kanji(MOTTO_FALL_AND_RISE.kanji.into());
    ui.set_motto_romaji(MOTTO_FALL_AND_RISE.romaji.into());
    ui.set_motto_meaning(MOTTO_FALL_AND_RISE.meaning.into());
    #[cfg(target_os = "ios")]
    ui.set_is_mobile(true);

    let direction = db
        .get_setting("card_direction")?
        .unwrap_or_else(|| "random".to_string());
    ui.set_card_direction(direction.into());
    ui.set_new_card_limit(new_card_limit(&db) as i32);
    refresh_deck_lists(&ui, &db);
    refresh_streak(&ui, &db);
    let db_ref = Rc::new(db);
    let card_shown_at: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
    let session: Rc<RefCell<Session>> = Rc::new(RefCell::new(Session::default()));

    // Keeps "today" correct and picks up the goal if the app stays open
    // across midnight.
    let streak_timer = slint::Timer::default();
    {
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        streak_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(30),
            move || {
                if let Some(ui) = ui_handle.upgrade() {
                    refresh_streak(&ui, db_ref.as_ref());
                }
            },
        );
    }

    ui.on_open_deck_select({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_status_message("".into());
                refresh_deck_lists(&ui, db_ref.as_ref());
                refresh_streak(&ui, db_ref.as_ref());
                ui.set_current_state(AppState::DeckSelect);
            }
        }
    });

    ui.on_change_deck({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        let card_shown_at = card_shown_at.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                record_study_time(db_ref.as_ref(), card_shown_at.as_ref());
                refresh_streak(&ui, db_ref.as_ref());
                ui.set_status_message("".into());
                refresh_deck_lists(&ui, db_ref.as_ref());
                ui.set_current_state(AppState::DeckSelect);
            }
        }
    });

    ui.on_select_deck({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        let card_shown_at = card_shown_at.clone();
        let session = session.clone();
        move |deck| {
            if let Some(ui) = ui_handle.upgrade() {
                let deck = deck.to_string();
                let (due, _, _) = db_ref.deck_counts(&deck, &today_key()).unwrap_or((0, 0, 0));
                if due == 0 {
                    start_session(&ui, db_ref.as_ref(), &deck, true, session.as_ref(), card_shown_at.as_ref());
                } else {
                    start_session(&ui, db_ref.as_ref(), &deck, false, session.as_ref(), card_shown_at.as_ref());
                }
            }
        }
    });

    ui.on_reveal_card({
        let ui_handle = ui.as_weak();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_is_revealed(true);
            }
        }
    });

    ui.on_rate_card({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        let card_shown_at = card_shown_at.clone();
        let session = session.clone();
        move |rating| {
            if let Some(ui) = ui_handle.upgrade() {
                let card_id = ui.get_current_card_id() as i64;
                if card_id <= 0 {
                    return;
                }
                let Ok(card) = db_ref.get_card(card_id) else {
                    return;
                };

                let mut state = card.state.clone();
                calculate_next_review(&mut state, rating_to_result(rating));
                let day = today_key();
                if let Err(err) = db_ref.record_review(&card, &state, rating, &day) {
                    eprintln!("Failed to record review for card {}: {err}", card_id);
                }

                {
                    let mut session = session.borrow_mut();
                    session.record(&card, rating);
                }

                // If the session was limited and we just finished the queue, auto-continue.
                if session.borrow().queue.is_empty() && !ui.get_status_message().to_string().contains("Keep learning") {
                    ui.set_status_message("Daily limit reached, continuing...".into());
                    auto_continue_session(&ui, db_ref.as_ref(), session.as_ref(), card_shown_at.as_ref());
                    return;
                }

                record_study_time(db_ref.as_ref(), card_shown_at.as_ref());
                refresh_streak(&ui, db_ref.as_ref());
                show_next_card(
                    &ui,
                    db_ref.as_ref(),
                    session.as_ref(),
                    card_shown_at.as_ref(),
                );
            }
        }
    });

    ui.on_keep_learning({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        let card_shown_at = card_shown_at.clone();
        let session = session.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let deck = if session.borrow().deck.is_empty() {
                    ui.get_selected_deck().to_string()
                } else {
                    session.borrow().deck.clone()
                };
                ui.set_status_message(format!("Learning all cards in {}", deck).into());
                start_session(
                    &ui,
                    db_ref.as_ref(),
                    &deck,
                    true,
                    session.as_ref(),
                    card_shown_at.as_ref(),
                );
            }
        }
    });

    ui.on_end_session({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                refresh_deck_lists(&ui, db_ref.as_ref());
                refresh_streak(&ui, db_ref.as_ref());
                ui.set_status_message("".into());
                ui.set_current_state(AppState::DeckSelect);
            }
        }
    });


    ui.on_open_settings({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_status_message("".into());
                ui.set_new_card_limit(new_card_limit(db_ref.as_ref()) as i32);
                ui.set_current_state(AppState::Settings);
            }
        }
    });

    ui.on_set_card_direction({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move |mode| {
            if let Some(ui) = ui_handle.upgrade() {
                let mode = mode.to_string();
                let _ = db_ref.set_setting("card_direction", &mode);
                ui.set_card_direction(mode.into());
                ui.set_status_message("Saved.".into());
            }
        }
    });

    ui.on_set_new_card_limit({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move |limit| {
            if let Some(ui) = ui_handle.upgrade() {
                let limit = limit.clamp(1, 500);
                if let Err(err) = db_ref.set_setting("new_card_limit", &limit.to_string()) {
                    ui.set_status_message(format!("Could not save limit: {err}").into());
                    return;
                }
                ui.set_new_card_limit(limit);
                refresh_deck_lists(&ui, db_ref.as_ref());
                ui.set_status_message(format!("New cards per day: {limit}").into());
            }
        }
    });

    ui.on_backup_database({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let stamp = Local::now().format("%Y-%m-%d_%H%M%S");
                let path = export_dir()
                    .join("backups")
                    .join(format!("shizuka-{stamp}.db"));
                match db_ref.backup_to(&path) {
                    Ok(()) => {
                        #[cfg(target_os = "macos")]
                        let _ = Command::new("open").arg("-R").arg(&path).spawn();
                        let where_text = if cfg!(target_os = "ios") {
                            "the Shizuka folder in the Files app"
                        } else if cfg!(target_os = "windows") {
                            "your Downloads\\Shizuka folder"
                        } else {
                            "~/Downloads/Shizuka/backups"
                        };
                        ui.set_status_message(format!("Backup saved to {where_text}.").into());
                    }
                    Err(err) => ui.set_status_message(format!("Backup failed: {err}").into()),
                }
            }
        }
    });

    ui.on_open_manage({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_status_message("".into());
                refresh_deck_lists(&ui, db_ref.as_ref());
                ui.set_current_state(AppState::ManageDecks);
            }
        }
    });

    ui.on_toggle_manage_deck({
        let ui_handle = ui.as_weak();
        move |idx| {
            if let Some(ui) = ui_handle.upgrade() {
                let model = ui.get_manage_decks();
                let count = model.row_count();
                let mut rows: Vec<ManageDeck> =
                    (0..count).filter_map(|i| model.row_data(i)).collect();
                if let Some(row) = rows.get_mut(idx as usize) {
                    row.selected = !row.selected;
                }
                ui.set_manage_decks(ModelRc::new(VecModel::from(rows)));
            }
        }
    });

    ui.on_bulk_export({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let selected = selected_manage_decks(&ui);
                if selected.is_empty() {
                    ui.set_status_message("Select one or more decks to export.".into());
                    return;
                }
                let mut paths = Vec::new();
                for deck in &selected {
                    match db_ref.export_deck_csv(deck, &export_dir()) {
                        Ok(path) => paths.push(path),
                        Err(err) => {
                            ui.set_status_message(
                                format!("Export failed ({}): {err}", deck).into(),
                            );
                            return;
                        }
                    }
                }
                #[cfg(target_os = "macos")]
                if let Some(first) = paths.first() {
                    let _ = Command::new("open").arg("-R").arg(first).spawn();
                }
                #[cfg(target_os = "windows")]
                if let Some(first) = paths.first() {
                    let _ = Command::new("explorer").arg("/select,").arg(first).spawn();
                }
                let exported_to = if cfg!(target_os = "ios") {
                    "the Shizuka folder in the Files app"
                } else if cfg!(target_os = "windows") {
                    "your Downloads\\Shizuka folder"
                } else {
                    "~/Downloads/Shizuka"
                };
                ui.set_status_message(
                    format!("Exported {} deck(s) to {}.", paths.len(), exported_to).into(),
                );
            }
        }
    });

    ui.on_bulk_hide({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let selected = selected_manage_decks(&ui);
                if selected.is_empty() {
                    ui.set_status_message("Select one or more decks to hide.".into());
                    return;
                }
                for deck in &selected {
                    let _ = db_ref.set_deck_hidden(deck, true);
                }
                refresh_deck_lists(&ui, db_ref.as_ref());
                ui.set_status_message(format!("Hidden {} deck(s).", selected.len()).into());
            }
        }
    });

    ui.on_bulk_unhide({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let selected = selected_manage_decks(&ui);
                if selected.is_empty() {
                    ui.set_status_message("Select one or more decks to show.".into());
                    return;
                }
                for deck in &selected {
                    let _ = db_ref.set_deck_hidden(deck, false);
                }
                refresh_deck_lists(&ui, db_ref.as_ref());
                ui.set_status_message(format!("Shown {} deck(s).", selected.len()).into());
            }
        }
    });

    ui.on_import_path_deck({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move |path| {
            if let Some(ui) = ui_handle.upgrade() {
                let path = path.to_string();
                match db_ref.import_deck_csv(std::path::Path::new(&path), None) {
                    Ok((name, n)) => {
                        refresh_deck_lists(&ui, db_ref.as_ref());
                        ui.set_status_message(format!("Imported {} ({} cards).", name, n).into());
                    }
                    Err(err) => ui.set_status_message(format!("Import failed: {err}").into()),
                }
            }
        }
    });

    ui.on_import_clipboard({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let deck_name = ui.get_import_deck_name().to_string();
                match read_clipboard() {
                    Some(text) if !text.trim().is_empty() => {
                        match db_ref.import_deck_csv_text(&text, &deck_name) {
                            Ok((name, n)) => {
                                refresh_deck_lists(&ui, db_ref.as_ref());
                                ui.set_status_message(
                                    format!("Imported {} ({} cards).", name, n).into(),
                                );
                            }
                            Err(err) => {
                                ui.set_status_message(format!("Import failed: {err}").into())
                            }
                        }
                    }
                    _ => ui.set_status_message("Clipboard is empty. Copy CSV text first.".into()),
                }
            }
        }
    });

    ui.on_import_folder({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        let db_path = db_path.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let dir = import_dir(&db_path);
                match db_ref.import_decks_from_dir(&dir) {
                    Ok(results) if results.is_empty() => {
                        ui.set_status_message(format!("No .csv files in {}", dir.display()).into());
                    }
                    Ok(results) => {
                        refresh_deck_lists(&ui, db_ref.as_ref());
                        let msg = results
                            .iter()
                            .map(|(n, c)| format!("{} ({})", n, c))
                            .collect::<Vec<_>>()
                            .join(", ");
                        ui.set_status_message(format!("Imported: {}", msg).into());
                    }
                    Err(err) => ui.set_status_message(format!("Import failed: {err}").into()),
                }
            }
        }
    });

    println!("Shizuka is running! Close the window to exit.");
    ui.run()?;

    // Credit the card that was on screen when the window closed.
    record_study_time(db_ref.as_ref(), card_shown_at.as_ref());
    let _ = sync_streak(db_ref.as_ref());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_card(id: i64, front: &str) -> Card {
        Card {
            id,
            deck: "Test".to_string(),
            front: front.to_string(),
            back: "answer".to_string(),
            reading: String::new(),
            notes: String::new(),
            introduced_on: None,
            state: CardState::default(),
        }
    }

    #[test]
    fn good_and_easy_ratings_do_not_repeat_the_card() {
        let card = test_card(1, "ねこ");
        let mut session = Session::default();

        session.record(&card, 2); // Good
        session.record(&card, 3); // Very good

        assert!(session.queue.is_empty());
        assert_eq!(session.reviewed, 2);
        assert_eq!(session.correct, 2);
        assert!(session.missed.is_empty());
    }

    #[test]
    fn bad_rating_requeues_and_counts_misses() {
        let card = test_card(7, "いぬ");
        let mut session = Session::default();

        session.record(&card, 0);
        session.record(&card, 0);

        assert_eq!(session.queue.len(), 2);
        assert_eq!(session.reviewed, 2);
        assert_eq!(session.correct, 0);
        assert_eq!(session.missed.get(&7).unwrap().1, 2);
        assert_eq!(session.missed.get(&7).unwrap().0, "いぬ");
    }

    #[test]
    fn interval_labels_cover_each_rating() {
        let state = CardState::default();
        let labels = interval_previews(&state);

        assert_eq!(labels.len(), 4);
        for label in &labels {
            assert!(!label.is_empty());
        }
    }

    #[test]
    fn formats_interval_units() {
        assert_eq!(format_interval(0), "<1m");
        assert_eq!(format_interval(1), "1m");
        assert_eq!(format_interval(10), "10m");
        assert_eq!(format_interval(60), "1h");
        assert_eq!(format_interval(1440), "1d");
        assert_eq!(format_interval(12 * 1440), "12d");
    }

    #[test]
    fn picks_the_japanese_side_to_speak() {
        let mut card = test_card(1, "cat");
        card.back = "ねこ".to_string();
        assert_eq!(speak_text(&card), "ねこ");
        assert!(contains_japanese("日本語"));
        assert!(!contains_japanese("hello"));
    }

    #[test]
    fn interval_preview_matches_scheduling() {
        let state = CardState::default();

        let labels = interval_previews(&state);
        let mut good = state.clone();
        calculate_next_review(&mut good, ReviewResult::Good);
        assert_eq!(labels[2], format_interval(good.interval));
    }
}

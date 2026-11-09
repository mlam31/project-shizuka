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
use rand::seq::SliceRandom;
use rand::Rng;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::Cell;
use std::env;
use std::path::PathBuf;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;
use std::rc::Rc;
use std::time::Instant;

use crate::core::{calculate_next_review, ReviewResult, Streak, DAILY_GOAL_SECONDS};
use crate::db::Database;
use crate::ui::{AppState, AppWindow, ManageDeck};

/// Longest stretch counted from a single card, so an idle app cannot farm
/// study time (and a streak) by simply being left open.
const MAX_CARD_SECONDS: u64 = 120;

fn rating_to_result(rating: i32) -> ReviewResult {
    match rating {
        0 => ReviewResult::Again, // Bad
        1 => ReviewResult::Hard,  // OK
        2 => ReviewResult::Good,  // Good
        _ => ReviewResult::Easy,  // Very good
    }
}

fn refresh_deck_lists(ui: &AppWindow, db: &Database) {
    let visible = db.list_decks().unwrap_or_default();
    let all = db.list_all_decks().unwrap_or_default();

    ui.set_deck_names(ModelRc::new(VecModel::from(
        visible.into_iter().map(|s| s.into()).collect::<Vec<slint::SharedString>>(),
    )));

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

    ui.set_streak_text(streak_text.into());
    ui.set_today_text(today_text.into());
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
    let day = Local::now().date_naive().format("%Y-%m-%d").to_string();
    if let Err(err) = db.add_study_seconds(&day, studied) {
        eprintln!("Failed to record study time: {err}");
    }
}

fn show_next_card(
    ui: &AppWindow,
    db: &Database,
    deck: &str,
    shown_at: &Cell<Option<Instant>>,
) -> Result<()> {
    shown_at.set(None);
    match db.get_all_cards(deck) {
        Ok(cards) if !cards.is_empty() => {
            let mut rng = rand::thread_rng();
            let current_id = ui.get_current_card_id() as i64;
            let choices: Vec<_> = if cards.len() > 1 {
                cards.iter().filter(|c| c.id != current_id).collect()
            } else {
                cards.iter().collect()
            };
            let card = (*choices.choose(&mut rng).unwrap()).clone();

            let direction = ui.get_card_direction().to_string();
            let reverse = match direction.as_str() {
                "character" => false,
                "pronunciation" => true,
                _ => rng.gen_bool(0.5),
            };
            let (prompt, answer) = if reverse {
                (card.back.clone(), card.front.clone())
            } else {
                (card.front.clone(), card.back.clone())
            };

            ui.set_selected_deck(deck.into());
            ui.set_current_card_id(card.id as i32);
            ui.set_card_front(prompt.into());
            ui.set_card_back(answer.into());
            ui.set_is_revealed(false);
            ui.set_current_state(AppState::Reviewing);
            shown_at.set(Some(Instant::now()));
        }
        Ok(_) => {
            ui.set_current_card_id(0);
            ui.set_card_front("No cards".into());
            ui.set_card_back("This deck is empty. Tap Decks to choose another.".into());
            ui.set_is_revealed(true);
            ui.set_selected_deck(deck.into());
            ui.set_current_state(AppState::Reviewing);
        }
        Err(err) => {
            eprintln!("Failed to load deck {}: {err}", deck);
        }
    }
    Ok(())
}

/// Directory where exported deck CSVs are written.
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
        eprintln!("Failed to create import folder {}: {err}", import_path.display());
    }

    let ui = AppWindow::new()?;
    #[cfg(target_os = "ios")]
    ui.set_is_mobile(true);
    let direction = db
        .get_setting("card_direction")?
        .unwrap_or_else(|| "random".to_string());
    ui.set_card_direction(direction.into());
    refresh_deck_lists(&ui, &db);
    refresh_streak(&ui, &db);
    let db_ref = Rc::new(db);
    let card_shown_at: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));

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
        move |deck| {
            if let Some(ui) = ui_handle.upgrade() {
                let deck = deck.to_string();
                let _ = show_next_card(
                    &ui,
                    db_ref.as_ref(),
                    &deck,
                    card_shown_at.as_ref(),
                );
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
        move |rating| {
            if let Some(ui) = ui_handle.upgrade() {
                let deck = ui.get_selected_deck().to_string();
                let card_id = ui.get_current_card_id() as i64;
                if card_id > 0 {
                    if let Ok(card) = db_ref.get_card(card_id) {
                        let mut state = card.state;
                        calculate_next_review(&mut state, rating_to_result(rating));
                        if let Err(err) = db_ref.update_card(card_id, &state) {
                            eprintln!("Failed to update card {}: {err}", card_id);
                        }
                    }
                }
                record_study_time(db_ref.as_ref(), card_shown_at.as_ref());
                refresh_streak(&ui, db_ref.as_ref());
                let _ = show_next_card(
                    &ui,
                    db_ref.as_ref(),
                    &deck,
                    card_shown_at.as_ref(),
                );
            }
        }
    });

    ui.on_open_settings({
        let ui_handle = ui.as_weak();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_status_message("".into());
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
                let mut rows: Vec<ManageDeck> = (0..count).filter_map(|i| model.row_data(i)).collect();
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
                            ui.set_status_message(format!("Export failed ({}): {err}", deck).into());
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

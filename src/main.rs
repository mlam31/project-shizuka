mod core;
mod db;
mod ui;

use anyhow::Result;
use slint::ComponentHandle;
use crate::ui::{AppWindow, AppState};
use crate::db::Database;
use crate::core::Card;

fn main() -> Result<()> {
    // 1. Initialize the database
    let db_path = "shizuka.db";
    let db = Database::new(db_path)?;

    // 2. Seed the database if it's empty
    let existing_cards = db.get_due_cards("Fundamentals")?;
    if existing_cards.is_empty() {
        println!("Seeding starter deck...");
        db.seed_starter_deck()?;
    }

    // 3. Create the UI
    let ui = AppWindow::new()?;

    // 4. Set up the logic (Simplified for now to get it running)
    // We use the handle directly within the closure to avoid the clone error.

    let ui_handle = ui.as_weak();
    let db_ref = std::sync::Arc::new(db);

    ui.on_start_review({
        let ui_handle = ui.as_weak();
        let db_ref = db_ref.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_current_state(AppState::Reviewing);
                ui.set_is_revealed(false);

                if let Ok(cards) = db_ref.get_due_cards("Fundamentals") {
                    if !cards.is_empty() {
                        let card = cards[0].clone();
                        ui.set_card_front(card.front.clone().into());
                        ui.set_card_back(card.back.clone().into());
                    }
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
        move |rating| {
            if let Some(ui) = ui_handle.upgrade() {
                // For now, we'll just print the rating and reset to welcome
                // to prove the UI interaction works.
                println!("Rated card: {}", rating);
                ui.set_current_state(AppState::Welcome);
            }
        }
    });

    println!("Shizuka is running! Close the window to exit.");
    ui.run()?;

    Ok(())
}

# Project Shizuka (静か) - Minimalist Japanese SRS Engine

## 🎯 Vision
To transform "dead time" (doomscrolling) into "intentional learning" time. Project Shizuka is an ultra-lightweight, high-performance Spaced Repetition System (SRS) designed for distraction-free Japanese language acquisition. The 
aesthetic is strictly Black & White (High Contrast/E-ink style) to minimize cognitive load and battery consumption.

## 🧠 Problem Statement
*   **The Addiction:** Modern social media apps are designed to trap users in infinite scrolls.
*   **The Friction:** Existing Anki implementations are often bloated, heavy, or difficult to use on mobile for quick sessions.
*   **The Solution:** A "Zen-mode" application that uses a high-performance Rust backend to provide instant, meaningful linguistic feedback.

## 🛠 Technical Stack
- **Language:** Rust (Core logic, performance, memory safety).
- **UI Framework:** [Slint](https://slint.dev/) (Lightweight, embedded-friendly, ideal for the B&W aesthetic).
- **Database:** SQLite (via `rusqlite`) for local, fast, ACID-compliant data persistence.
- **Algorithm:** Custom implementation of the **SM-2 (Spaced Repetition)** algorithm.
- **AI Content Engine (Local):** 
    - **Inference:** Ollama or MLX (running on Mac M-series).
    - **Models:** Llama 3 or Mistral.
    - **Purpose:** "Sentence Mining" (converting raw text into flashcards) and grammatical decomposition.
- **Target Hardware:** 
    - *Development:* Mac M-series.
    - *Deployment Goal:* Cross-compiled Rust binary for mobile/embedded targets.

## 🏗 System Architecture

### 1. The Core Engine (Rust Crates)
- `shizuka_core`: The SM-2 algorithm implementation and business logic.
- `shizuka_db`: SQLite schema management and CRUD operations.
- `shizuka_ui`: Slint-based interface definition (B&W minimalist theme).
- `shizuka_ai_bridge`: A CLI tool/service to interface with local LLMs to generate flashcard content.

### 2. Database Schema (Draft)
- `cards`: id, front (kanji/kana), back (meaning/reading), ease_factor, interval, repetitions, next_review_date.
- `sentences`: id, text, translation, grammatical_breakdown.
- `tags`: id, name.

### 3. AI Workflow (The "Content Factory")
1. **Input:** Raw Japanese text (news, manga, or user input).
2. **Process:** Local LLM analyzes text $\rightarrow$ Extracts vocabulary $\rightarrow$ Breaks down grammar $\rightarrow$ Generates example sentences.
3. **Output:** Structured JSON/SQL ready to be ingested by the SQLite database.

## 🚀 Development Roadmap

### Phase 1: The Foundation (CLI & Logic)
- [ ] Initialize Rust workspace.
- [ ] Implement SM-2 algorithm logic.
- [ ] Set up SQLite schema and basic data persistence.
- [ ] Create a CLI version for testing card retrieval.

### Phase 2: The Interface (Slint)
- [ ] Design the "Black & White" UI specification in `.slint`.
- [ ] Implement the "Flashcard View" (Front $\rightarrow$ Reveal $\rightarrow$ Rating).
- [ ] Implement the "Daily Review" loop.

### Phase 3: The AI Engine
- [ ] Create a Python/Rust script to communicate with Ollama/MLX.
- [ ] Prompt engineering for "Grammar Decomposition".
- [ ] Automate the import of generated cards into the SQLite DB.

### Phase 4: Mobile Optimization
- [ ] Explore cross-compilation paths for mobile targets.
- [ ] Optimize UI for small screens (iPhone 13 mini dimensions).

## 🤖 AI Development Instructions (For Hermes Agent & Qwen 2.5 Coder)
*When generating code, follow these principles:*
1. **Strict Rust Idioms:** Use `clippy` standards. Favor memory safety and zero-cost abstractions.
2. **Minimalism:** Every line of UI code must serve a purpose. No decorative elements; only functional typography.
3. **Modularity:** Keep the `core` logic decoupled from the `ui` to allow for easy testing and future platform migrations.
4. **Error Handling:** Use `Result` and `Option` properly. No `unwrap()` in production code.
5. **Documentation:** Every public function must have a doc comment explaining its role in the SRS lifecycle.

## 📂 Repository Structure
```text
/shizuka
├── Cargo.toml
├── src/
│   ├── main.rs          # Entry point
│   ├── core/            # SM-2 & Logic
│   ├── db/              # SQLite management
│   └── ui/              # Slint definitions
├── assets/              # Fonts & Styles
├── scripts/             # AI Content generation scripts
└── docs/                # Architecture notes
```
```

***

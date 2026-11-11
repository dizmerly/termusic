mod anim;
mod app;
mod art;
mod auth;
mod cmd;
mod commands;
mod config;
mod history;
mod net;
mod player;
mod queue;
mod remote;
mod settings;
mod theme;
mod ui;
mod youtube;

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::event::{
    self, Event, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "termusic", about = "Terminal YouTube music player")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Sign in with Google
    Login,
    /// Remove stored credentials
    Logout,
    /// Search YouTube and print results
    Search { query: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        None => run_tui(),
        Some(Cmd::Login) => {
            auth::login(|url| println!("Open this page to sign in:\n{url}")).await?;
            println!("logged in");
            Ok(())
        }
        Some(Cmd::Logout) => {
            auth::logout()?;
            println!("logged out");
            Ok(())
        }
        Some(Cmd::Search { query }) => {
            for (i, t) in youtube::search(&query).await?.iter().enumerate() {
                println!("{:>2}. {} — {}", i + 1, t.title, t.channel);
            }
            Ok(())
        }
    }
}

fn run_tui() -> Result<()> {
    let mut terminal = ratatui::init();
    // Lets terminals with the kitty protocol report Ctrl-Backspace distinctly.
    let enhanced = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        let _ = execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    let mut app = app::App::new();
    (app.art_path, app.picker) = art::detect();
    let res = (|| -> Result<()> {
        while !app.quit {
            app.tick();
            terminal.draw(|f| ui::draw(f, &mut app))?;
            let wait = if app.animating() {
                anim::FRAME
            } else {
                Duration::from_millis(200)
            };
            if event::poll(wait)?
                && let Event::Key(k) = event::read()?
                && k.kind == KeyEventKind::Press
            {
                app.handle_key(k);
            }
        }
        Ok(())
    })();
    if enhanced {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();
    app.shutdown();
    res
}

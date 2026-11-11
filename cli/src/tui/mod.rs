//! The terminal UI: ratatui on crossterm, driven by one message channel.
//!
//! Three kinds of producers feed [`app::Msg`]s into it: a thread reading
//! terminal input, a task holding the live event stream (`/api/stream`) and
//! short-lived tasks for HTTP requests. The UI loop itself never waits on the
//! network.

mod app;
mod ui;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::api::Client;
use crate::types::StreamMessage;
use app::{App, Conn, Msg};

/// Puts the terminal back however the UI ends. `ratatui::init` also installs a
/// panic hook that restores it before the panic message is printed.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

pub async fn run(client: Client) -> Result<()> {
    let (tx, mut rx) = unbounded_channel();
    let mut app = App::new(client.clone(), tx.clone());

    let mut terminal = ratatui::init();
    let _restore = Restore;

    let stop = Arc::new(AtomicBool::new(false));
    spawn_input(tx.clone(), Arc::clone(&stop));
    spawn_ticker(tx.clone());
    let stream = tokio::spawn(follow_stream(client, tx));
    app.start();

    let result = loop {
        if let Err(err) = terminal.draw(|frame| ui::render(frame, &mut app)) {
            break Err(err.into());
        }
        let Some(msg) = rx.recv().await else {
            break Ok(());
        };
        app.update(msg);
        // Handle whatever else is queued before drawing again.
        while let Ok(msg) = rx.try_recv() {
            app.update(msg);
        }
        if app.quit {
            break Ok(());
        }
    };

    stop.store(true, Ordering::Relaxed);
    stream.abort();
    result
}

/// Forwards terminal events. Blocking reads live on their own thread.
fn spawn_input(tx: UnboundedSender<Msg>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match event::poll(Duration::from_millis(200)) {
                Ok(true) => match event::read() {
                    Ok(event) => {
                        if tx.send(Msg::Input(event)).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
}

fn spawn_ticker(tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if tx.send(Msg::Tick).is_err() {
                break;
            }
        }
    });
}

/// Keeps the live stream connected, reconnecting with a growing delay.
async fn follow_stream(client: Client, tx: UnboundedSender<Msg>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if let Ok(mut stream) = client.stream().await {
            backoff = Duration::from_secs(1);
            let _ = tx.send(Msg::Conn(Conn::Live));
            while let Ok(Some(value)) = stream.next().await {
                if let Ok(message) = serde_json::from_value::<StreamMessage>(value)
                    && tx.send(Msg::Stream(message)).is_err()
                {
                    return;
                }
            }
        }
        if tx.send(Msg::Conn(Conn::Reconnecting)).is_err() {
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(15));
    }
}

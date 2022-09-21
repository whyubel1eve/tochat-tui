use crossterm::{
    event::{self, Event, KeyCode},
};

use tokio::sync::mpsc::{Sender, Receiver};
use std::{io, sync::{Arc, Mutex}};
use tui::{
    backend::Backend,
    Terminal,
};
use super::{InputMode, ui::ui, App};

use chrono::prelude::*;



pub async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: App, 
    tx1: Sender<String>,
    mut rx2: Receiver<String>,
    name: &String
) -> io::Result<()> {

    let app = Arc::new(Mutex::new(app));
    let app_clone = app.clone();

    tokio::spawn(async move {
        loop {
            let msg = rx2.recv().await.unwrap();
            app_clone.lock().unwrap().messages.push(msg);
        }
    });

    loop {
        terminal.draw(|f| ui(f, &app.lock().unwrap()))?;

        if let Event::Key(key) = event::read()? {
            match app.lock().unwrap().input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('i') => {
                        app.lock().unwrap().input_mode = InputMode::Editing;
                    }
                    KeyCode::Char('q') => {
                        return Ok(());
                    }
                    _ => {}
                },
                InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        tx1.send(format!("{},{}", app.lock().unwrap().input.clone(), name)).await.unwrap();
                        app.lock().unwrap().messages.push(format!("{}, {}\r\n{}", 
                            *name, 
                            Local::now().format("%H:%M:%S").to_string(), 
                            app.lock().unwrap().input.drain(..).collect::<String>()));
                    }
                    KeyCode::Char(c) => {
                        app.lock().unwrap().input.push(c);
                    }
                    KeyCode::Backspace => {
                        app.lock().unwrap().input.pop();
                    }
                    KeyCode::Esc => {
                        app.lock().unwrap().input_mode = InputMode::Normal;
                    }
                    _ => {}
                },
            }
        }
    }
}
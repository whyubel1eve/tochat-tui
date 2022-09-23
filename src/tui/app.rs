use crossterm::{
    event::{self, Event, KeyCode, poll},
};

use instant::Duration;
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

    // crossed thread data
    let app = Arc::new(Mutex::new(app));
    let app_clone = app.clone();


    tokio::spawn(async move {
        loop {
            while let Some(msg) = rx2.recv().await {
                let mut lock = app_clone.lock().unwrap();
                (*lock).messages.items.push(msg);
                let len = (*lock).messages.items.len() - 1;
                (*lock).messages.state.select(Some(len));
            }
        }
    });

    loop {
        terminal.draw(|f| ui(f, &mut app.lock().unwrap()))?;
        
        // flush every 50 millis, avoid blocking
        if poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                let mut lock = app.lock().unwrap();
                match (*lock).input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('i') => {
                            (*lock).input_mode = InputMode::Editing;
                        }
                        KeyCode::Char('q') => {
                            return Ok(());
                        }
                        KeyCode::Left => (*lock).messages.unselect(),
                        KeyCode::Down => (*lock).messages.next(),
                        KeyCode::Up => (*lock).messages.previous(),
                        KeyCode::Char('j') => (*lock).messages.next(),
                        KeyCode::Char('k') => (*lock).messages.previous(),
                        KeyCode::Home => (*lock).messages.home(),
                        KeyCode::End => (*lock).messages.end(),
                        _ => {}
                    },
                    InputMode::Editing => match key.code {
                        KeyCode::Enter => {
                            tx1.send(format!("{},{}",  (*lock).input.clone(), name)).await.unwrap();
                            let s = format!("{} {} - {}", 
                                *name, 
                                Local::now().format("%H:%M:%S").to_string(), 
                                (*lock).input.drain(..).collect::<String>());
                            (*lock).messages.items.push(s);
                            let len = (*lock).messages.items.len() - 1;
                            (*lock).messages.state.select(Some(len));
                        }
                        KeyCode::Char(c) => {
                            (*lock).input.push(c);
                        }
                        KeyCode::Backspace => {
                            (*lock).input.pop();
                        }
                        KeyCode::Esc => {
                            (*lock).input_mode = InputMode::Normal;
                        }
                        _ => {}
                    },
                }
            }
        } 
    }
}
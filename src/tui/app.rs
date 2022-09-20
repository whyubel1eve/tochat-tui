use crossterm::{
    event::{self, Event, KeyCode},
};
use async_std::task::block_on;
use futures::StreamExt;
use libp2p::Swarm;
use std::io;
use tui::{
    backend::Backend,
    Terminal,
};
use colorful::Colorful;
use colorful::Color;

use crate::network::connection::{self, Behaviour};

use super::{InputMode, ui::ui, App};
use libp2p::gossipsub::{IdentTopic as Topic, GossipsubEvent};
use libp2p::swarm::SwarmEvent;

use chrono::prelude::*;



pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App, 
    mut swarm: Swarm<Behaviour>, 
    topic: Topic, 
    name: &String
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('i') => {
                        app.input_mode = InputMode::Editing;
                    }
                    KeyCode::Char('q') => {
                        return Ok(());
                    }
                    _ => {}
                },
                InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        swarm.behaviour_mut()
                            .gossip
                            .publish(topic.clone(),  format!("{},{}", app.input.clone(), name).as_bytes())
                            .expect("publish error");
                        app.messages.push(format!("{}, {}\r\n{}", 
                            (*name.clone()).color(Color::LightCyan), 
                            Local::now().format("%H:%M:%S").to_string().color(Color::LightCyan), 
                            app.input.drain(..).collect::<String>().color(Color::LightCyan)));
                    }
                    KeyCode::Char(c) => {
                        app.input.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                    }
                    _ => {}
                },
            }
        }

        // receive remote_messages
        let event = block_on(swarm.select_next_some());
        match event {
            SwarmEvent::Behaviour(connection::Event::Gossip(GossipsubEvent::Message{
                propagation_source: _,
                message_id: _,
                message,
            })) => {
                let message = String::from_utf8_lossy(&message.data);
                let tokens:Vec<&str> = message.split(",").collect();
                let content = tokens[0];
                let remote_name = tokens[1];

                app.messages.push(format!("{}, {}\r\n{}", 
                            remote_name.color(Color::LightCyan), 
                            Local::now().format("%H:%M:%S").to_string().color(Color::LightCyan), 
                            content.color(Color::LightCyan)));
            }
            _ => {}
        }
    }
}
use std::{error::Error, io};

use chrono::Local;
use crossterm::{
    terminal::{enable_raw_mode, EnterAlternateScreen, disable_raw_mode, LeaveAlternateScreen},
    execute,
    event::{EnableMouseCapture, DisableMouseCapture, poll, Event, self, KeyCode}};
use instant::Duration;
use tui::{
    Terminal,
    backend::{CrosstermBackend, Backend},
    layout::{Direction, Constraint, Layout},
    Frame,
    style::{Modifier, Style, Color},
    text::{Span, Spans, Text},
    widgets::{Paragraph, Block, ListItem, List, Borders, ListState}
};

pub enum InputMode {
    Normal,
    Editing,
}

pub struct StatefulList<T> {
    pub state: ListState,
    pub items: Vec<T>,
}

impl<T> StatefulList<T> {
    fn with_items(items: Vec<T>) -> StatefulList<T> {
        StatefulList {
            state: ListState::default(),
            items,
        }
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    i
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    i
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn home(&mut self) {
        self.state.select(Some(0));
    }

    fn end(&mut self) {
        self.state.select(Some(self.items.len() - 1));
    }

    fn unselect(&mut self) {
        self.state.select(None);
    }
}

pub struct App {
    /// Current value of the input box
    pub input: String,
    /// Current input mode
    pub input_mode: InputMode,
    /// History of recorded messages
    pub messages: StatefulList<String>,

}

impl Default for App {
    fn default() -> App {
        App {
            input: String::new(),
            input_mode: InputMode::Normal,
            messages: StatefulList::with_items(Vec::new()),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let app = App::default();
    let res = run_app(&mut terminal, app);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;


    
    if let Err(err) = res {
        println!("{:?}", err)
    }
    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App, 
) -> io::Result<()> {

    loop {
        terminal.draw(|f| ui(f, &mut app))?;
        
        // flush every 50 millis, avoid blocking
        if poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match app.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('i') => {
                            app.input_mode = InputMode::Editing;
                        }
                        KeyCode::Char('q') => {
                            return Ok(());
                        }
                        KeyCode::Left => app.messages.unselect(),
                        KeyCode::Down => app.messages.next(),
                        KeyCode::Up => app.messages.previous(),
                        KeyCode::Char('j') => app.messages.next(),
                        KeyCode::Char('k') => app.messages.previous(),
                        KeyCode::Home => app.messages.home(),
                        KeyCode::End => app.messages.end(),
                        _ => {}
                    },
                    InputMode::Editing => match key.code {
                        KeyCode::Enter => {
                            let s = format!("{}- {}",
                                Local::now().format("%H:%M:%S").to_string(), 
                                app.input.drain(..).collect::<String>());
                            app.messages.items.push(s);
                            app.messages.state.select(Some(app.messages.items.len() - 1));
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
        } 
    }
}

pub fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Percentage(80), Constraint::Percentage(20)].as_ref())
        .split(f.size());

    let top_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(10), Constraint::Percentage(90)].as_ref())
        .split(chunks[0]);

    let (msg, style) = match app.input_mode {
        InputMode::Normal => (
            vec![
                Span::raw("Press "),
                Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to exit, "),
                Span::styled("i", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to start editing."),
            ],
            Style::default().add_modifier(Modifier::RAPID_BLINK),
        ),
        InputMode::Editing => (
            vec![
                Span::raw("Press "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to stop editing, "),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to send the message"),
            ],
            Style::default(),
        ),
    };
    let mut text = Text::from(Spans::from(msg));
    text.patch_style(style);
    let help_message = Paragraph::new(text).block(Block::default());
    f.render_widget(help_message, top_chunks[0]);

    // messages display area
    let messages: Vec<ListItem> = app
        .messages
        .items
        .iter()
        .enumerate()
        .map(|(_, m)| {
            let c: Vec<_> = m.split("-").collect();
            let content = vec![
                Spans::from(Span::styled(c[0], Style::default().fg(Color::White)), ),
                Spans::from(Span::styled(c[1], Style::default().fg(Color::LightYellow))),
            ];
            ListItem::new(content)
        })
        .collect();
    let messages =
        List::new(messages)
            .block(Block::default().borders(Borders::ALL).title("Messages"))
            .highlight_style(
                Style::default()
                .bg(Color::Rgb(40, 40, 40)),
            );
    f.render_stateful_widget(messages, top_chunks[1], &mut app.messages.state);

    // input area
    let input = Paragraph::new(app.input.as_ref())
        .style(match app.input_mode {
            InputMode::Normal => Style::default(),
            InputMode::Editing => Style::default().fg(Color::Yellow),
        })
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[1]);
    match app.input_mode {
        InputMode::Normal =>
            // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
            {}

        InputMode::Editing => {
            // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
            f.set_cursor(
                // Put cursor past the end of the input text
                chunks[1].x + app.input.len() as u16 + 1,
                // Move one line down, from the border to the input line
                chunks[1].y + 1,
            )
        }
    }
}

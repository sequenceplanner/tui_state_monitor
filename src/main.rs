use futures::{Stream, StreamExt};
use r2r::QosProfile;
use serde::Deserialize;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::{io, time::Duration};

use crossterm::event::{self, Event as CEvent, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;

#[derive(Debug, Deserialize)]
struct InterfaceState {
    name: String,
    interface_type: String,
    state: String,
}

#[derive(Debug, Clone)]
enum State {
    Active,
    Inactive,
}

impl Default for State {
    fn default() -> Self {
        State::Inactive
    }
}

#[derive(Debug, Clone, Default)]
struct App {
    server_states: Vec<State>,
    publisher_states: Vec<State>,
    subscriber_states: Vec<State>,
}

impl App {
    fn new() -> App {
        App {
            server_states: vec![State::Inactive; 0],
            publisher_states: vec![State::Inactive; 0],
            subscriber_states: vec![State::Inactive; 0],
        }
    }

    fn update_state(self, interface: InterfaceState) -> App {
        let state = if interface.state == "Active" {
            State::Active
        } else {
            State::Inactive
        };

        let name = interface.name;
        let mut new_app = self.clone();
        match interface.interface_type.as_str() {
            "server" => {
                new_app.server_states =
                    App::update_specific_state(new_app.server_states, &name, state)
            }
            "publisher" => {
                new_app.publisher_states =
                    App::update_specific_state(new_app.publisher_states, &name, state)
            }
            "subscriber" => {
                new_app.subscriber_states =
                    App::update_specific_state(new_app.subscriber_states, &name, state)
            }
            _ => {}
        }
        new_app
    }

    fn update_specific_state(mut states: Vec<State>, name: &str, state: State) -> Vec<State> {
        if let Some(pos) = name
            .split_whitespace()
            .last()
            .and_then(|n| n.parse::<usize>().ok())
        {
            if pos > 0 {
                if pos > states.len() {
                    states.resize(pos, State::Inactive); // Add new elements if necessary
                }
                states[pos - 1] = state;
            }
        }
        states
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let ctx = r2r::Context::create()?;
    let node = r2r::Node::create(ctx, "monitor", "")?;
    let arc_node = Arc::new(Mutex::new(node));

    let shared_app = Arc::new(Mutex::new(App::new()));

    let arc_node_clone: Arc<Mutex<r2r::Node>> = arc_node.clone();
    let shared_app_clone = shared_app.clone();
    tokio::task::spawn(async move {
        spawn_subscriber(arc_node_clone, &shared_app_clone)
            .await
            .unwrap()
    });

    let shared_app_clone = shared_app.clone();
    tokio::task::spawn(async move { spawn_monitor(&shared_app_clone).await.unwrap() });

    let arc_node_clone: Arc<Mutex<r2r::Node>> = arc_node.clone();
    let handle = std::thread::spawn(move || loop {
        arc_node_clone
            .lock()
            .unwrap()
            .spin_once(std::time::Duration::from_millis(1000));
    });

    handle.join().unwrap();

    Ok(())
}

async fn spawn_subscriber(
    arc_node: Arc<Mutex<r2r::Node>>,
    shared_app: &Arc<Mutex<App>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = arc_node
        .lock()
        .unwrap()
        .subscribe::<r2r::std_msgs::msg::String>("/monitored_state", QosProfile::default())?;

    let shared_app_clone = shared_app.clone();
    tokio::task::spawn(async move {
        match subscriber_callback(subscriber, &shared_app_clone).await {
            Ok(()) => (),
            Err(e) => r2r::log_error!("monitor", "Monitor subscriber failed with: '{}'.", e),
        };
    });
    Ok(())
}

async fn subscriber_callback(
    mut subscriber: impl Stream<Item = r2r::std_msgs::msg::String> + Unpin,
    shared_app: &Arc<Mutex<App>>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        match subscriber.next().await {
            Some(msg) => {
                let shared_app_local = shared_app.lock().unwrap().clone();
                let data: Result<InterfaceState, _> = serde_json::from_str(&msg.data);
                if let Ok(interface_state) = data {
                    *shared_app.lock().unwrap() = shared_app_local.update_state(interface_state);
                };
            }
            None => {
                r2r::log_error!("monitor", "AGV 1 state subscriber did not get the message?");
            }
        }
    }
}

async fn spawn_monitor(shared_app: &Arc<Mutex<App>>) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        let app = shared_app.lock().unwrap().clone();
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)].as_ref())
                .split(f.size());

            let column_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(33),
                        Constraint::Percentage(33),
                        Constraint::Percentage(34),
                    ]
                    .as_ref(),
                )
                .split(chunks[0]);

            let render_state_list = |title, states: &Vec<State>| {
                let items: Vec<ListItem> = states
                    .iter()
                    .enumerate()
                    .map(|(i, state)| {
                        // let state_text = match state {
                        //     State::Active => "Active",
                        //     State::Inactive => "Inactive",
                        // };
                        let item = ListItem::new(format!("{} {}", title, i + 1));
                        let style = match state {
                            State::Active => Style::default().fg(Color::Green),
                            State::Inactive => Style::default().fg(Color::Red),
                        };
                        item.style(style)
                    })
                    .collect();

                List::new(items).block(Block::default().borders(Borders::ALL).title(title))
            };

            f.render_widget(
                render_state_list("Server", &app.server_states),
                column_chunks[0],
            );
            f.render_widget(
                render_state_list("Publisher", &app.publisher_states),
                column_chunks[1],
            );
            f.render_widget(
                render_state_list("Subscriber", &app.subscriber_states),
                column_chunks[2],
            );

            let info_text = "q - quit";
            let info = Paragraph::new(info_text)
                .block(Block::default().borders(Borders::ALL).title("Help"))
                .style(Style::default().fg(Color::Black).bg(Color::White));

            f.render_widget(info, chunks[1]);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let CEvent::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

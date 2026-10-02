pub mod app;
pub mod events;

use crate::ai::AiRequestConfig;
use crate::cli::ResolvedConnection;
use crate::storage::{load_connection_store, load_settings_store};
use crate::tui::app::{CryoDbTui, InsertMode, LoginPane, Mode, Pane, PromptMode, SettingsTab};
use crate::tui::events::{Event, EventHandler};
use crate::{ConnectionStore, StoredConnection};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Row, Table, Tabs},
};
use std::io;
use std::time::Duration;
use tui_textarea::TextArea;

pub async fn run_tui(
    resolved: Option<ResolvedConnection>,
    max_rows: Option<usize>,
) -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (settings, _, _) = load_settings_store();
    let settings_ref = settings.clone();
    let (connection_store, _) = load_connection_store();
    let initial_info = resolved.as_ref().map(|resolved| {
        let mut info = resolved.info.clone();
        if info.database.trim().is_empty()
            && let Some(database) = &resolved.database
        {
            info.database = database.clone();
        }
        info
    });
    let mut app = CryoDbTui::new(
        initial_info,
        recent_items(&connection_store),
        favorite_items(&connection_store),
        max_rows.unwrap_or(settings.table_query_limit),
    );
    if let Some(resolved) = resolved {
        app.pool = Some(resolved.pool);
    }
    app.ai_config = Some(AiRequestConfig {
        provider: settings.ai_provider,
        endpoint: settings.ai_endpoint,
        model: settings.ai_model,
        api_key: settings.ai_api_key,
        temperature: None,
        cli_command: settings.ai_cli_command,
    });

    if app.pool.is_some() {
        if app.mode == Mode::Prompt(PromptMode::Database) {
            app.refresh_databases().await;
        } else {
            app.refresh_tables().await;
        }
    }

    let mut events = EventHandler::new(Duration::from_millis(100));

    loop {
        terminal.draw(|f| {
            let root = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(f.size());

            if login_screen(&app) {
                render_login(f, root[0], &mut app);
                render_status(f, root[1], &app);
                if app.mode == Mode::Help {
                    render_help(f, f.size());
                }
                if app.mode == Mode::Prompt(PromptMode::Search) {
                    let area = centered_rect(60, 20, f.size());
                    f.render_widget(Clear, area);
                    let search_block = Block::default()
                        .borders(Borders::ALL)
                        .title(" Search (Enter: ok, Esc: cancel) ");
                    let search_area = Layout::default()
                        .margin(2)
                        .constraints([Constraint::Length(3)])
                        .split(area)[0];
                    f.render_widget(search_block, area);
                    app.search_editor
                        .set_block(Block::default().borders(Borders::ALL).title(" / "));
                    f.render_widget(app.search_editor.widget(), search_area);
                }
                return;
            }

            let main = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(24), Constraint::Percentage(76)])
                .split(root[0]);

            let workspace = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(10)])
                .split(main[1]);

            let explorer_style = pane_style(&app, Pane::Explorer);
            if explorer_shows_connections(&app) {
                let connections: Vec<ListItem> = app
                    .connections
                    .iter()
                    .map(|(label, _)| ListItem::new(format!(" {}", label)))
                    .collect();
                let explorer = List::new(connections)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(pane_title(&app, Pane::Explorer, "Explorer: Connections"))
                            .border_style(explorer_style),
                    )
                    .highlight_style(
                        Style::default()
                            .bg(Color::DarkGray)
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol(">> ");
                f.render_stateful_widget(explorer, main[0], &mut app.connection_state);
            } else {
                let search_suffix = if app.search_query().trim().is_empty() {
                    String::new()
                } else {
                    format!(" filtered: {}", app.filtered_tables().len())
                };
                let tables: Vec<ListItem> = app
                    .filtered_tables()
                    .iter()
                    .map(|t| ListItem::new(format!(" 󱘖 {}", t)))
                    .collect();
                let explorer = List::new(tables)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(pane_title(
                                &app,
                                Pane::Explorer,
                                &format!("Explorer{}", search_suffix),
                            ))
                            .border_style(explorer_style),
                    )
                    .highlight_style(
                        Style::default()
                            .bg(Color::DarkGray)
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol(">> ");
                f.render_stateful_widget(explorer, main[0], &mut app.sidebar_state);
            }

            let view_style = pane_style(&app, Pane::View);
            let filtered_rs = app.filtered_results();
            let rs = filtered_rs.as_ref().or(app.results.as_ref());
            if let Some(rs) = rs {
                let col_offset = app.results_column_scroll;
                let remaining_cols = rs.columns.len().saturating_sub(col_offset);
                let visible_cols = remaining_cols
                    .min(((workspace[0].width.saturating_sub(4) as usize) / 18).max(1));
                let visible_rows = workspace[0].height.saturating_sub(3) as usize;
                let header_cells = rs
                    .columns
                    .iter()
                    .skip(col_offset)
                    .take(visible_cols)
                    .map(|c| c.as_str());
                let header = Row::new(header_cells)
                    .style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                    .height(1);
                let rows = rs
                    .rows
                    .iter()
                    .skip(app.results_scroll)
                    .take(visible_rows)
                    .map(|row| {
                        let cells = row
                            .iter()
                            .skip(col_offset)
                            .take(visible_cols)
                            .map(|c| c.as_str());
                        Row::new(cells).height(1)
                    });
                let widths = (0..visible_cols)
                    .map(|_| Constraint::Min(18))
                    .collect::<Vec<_>>();
                let title = if visible_cols > 0 {
                    format!(
                        "View: Cols {}-{} / {}",
                        col_offset + 1,
                        col_offset + visible_cols,
                        rs.columns.len()
                    )
                } else {
                    String::from("View")
                };
                let table = Table::new(rows, &widths).header(header).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(pane_title(&app, Pane::View, &title))
                        .border_style(view_style),
                );
                f.render_widget(table, workspace[0]);
            } else {
                let text = if app.pool.is_none() {
                    if app.connections.is_empty() {
                        "No saved connections found"
                    } else {
                        "Select a saved connection with Enter"
                    }
                } else {
                    "Open a table from Explorer or run a query"
                };
                let view = Paragraph::new(text).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(pane_title(&app, Pane::View, "View"))
                        .border_style(view_style),
                );
                f.render_widget(view, workspace[0]);
            }

            let query_style = pane_style(&app, Pane::Query);
            let query_title = if app.pane == Pane::Query {
                " Query * "
            } else {
                " Query "
            };
            app.query_editor.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(query_title)
                    .border_style(query_style),
            );
            f.render_widget(app.query_editor.widget(), workspace[1]);

            let status_text = if app.mode == Mode::Command {
                format!(" :{}", app.command_editor.lines().join(""))
            } else if app.pool.is_some() {
                format!(
                    " {} | {}{} | {} | {} | DB: {}",
                    app.mode.label(),
                    app.pane.label(),
                    if app.window_prefix { " | Ctrl-w" } else { "" },
                    app.status_message,
                    app.connection.driver,
                    app.connection.database
                )
            } else {
                format!(
                    " {} | {}{} | {}",
                    app.mode.label(),
                    app.pane.label(),
                    if app.window_prefix { " | Ctrl-w" } else { "" },
                    app.status_message
                )
            };
            let status = Paragraph::new(status_text)
                .block(Block::default().borders(Borders::ALL).title(" Status "));
            f.render_widget(status, root[1]);

            if app.mode == Mode::Prompt(PromptMode::Ai) {
                let area = centered_rect(60, 30, f.size());
                f.render_widget(Clear, area);
                app.ai_prompt_editor.set_block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" AI Prompt (Enter: generate, Esc: normal) "),
                );
                f.render_widget(app.ai_prompt_editor.widget(), area);
            }

            if app.mode == Mode::Prompt(PromptMode::Search) {
                let area = centered_rect(60, 20, f.size());
                f.render_widget(Clear, area);
                let search_block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Search (Enter: ok, Esc: cancel) ");
                let search_area = Layout::default()
                    .margin(2)
                    .constraints([Constraint::Length(3)])
                    .split(area)[0];
                f.render_widget(search_block, area);
                app.search_editor
                    .set_block(Block::default().borders(Borders::ALL).title(" / "));
                f.render_widget(app.search_editor.widget(), search_area);
            }

            if app.mode == Mode::Prompt(PromptMode::Database) {
                let area = centered_rect(40, 50, f.size());
                f.render_widget(Clear, area);
                let dbs: Vec<ListItem> = app
                    .databases
                    .iter()
                    .map(|db| ListItem::new(format!(" 󱘖 {}", db)))
                    .collect();
                let db_list = List::new(dbs)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Select Database (j/k, Enter, Esc) "),
                    )
                    .highlight_style(
                        Style::default()
                            .bg(Color::DarkGray)
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol(">> ");
                f.render_stateful_widget(db_list, area, &mut app.db_picker_state);
            }

            if app.mode == Mode::Prompt(PromptMode::Settings) {
                let area = centered_rect(70, 70, f.size());
                f.render_widget(Clear, area);
                let outer_block = Block::default().borders(Borders::ALL).title(" Settings ");
                f.render_widget(outer_block, area);

                let settings_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(2)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(5),
                        Constraint::Length(3),
                    ])
                    .split(area);

                let tab_titles = vec![" General ", " Database ", " AI "];
                let active_tab_index = match app.settings_tab {
                    SettingsTab::General => 0,
                    SettingsTab::Database => 1,
                    SettingsTab::AI => 2,
                };
                let tabs = Tabs::new(tab_titles)
                    .block(Block::default().borders(Borders::BOTTOM))
                    .select(active_tab_index)
                    .style(Style::default().fg(Color::DarkGray))
                    .highlight_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    );
                f.render_widget(tabs, settings_layout[0]);

                let content_area = settings_layout[1];
                match app.settings_tab {
                    SettingsTab::General => {
                        let rows = Layout::default()
                            .constraints([Constraint::Length(3), Constraint::Min(0)])
                            .split(content_area);
                        let limit_style = if app.settings_focus_index == 0 {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        };
                        app.gen_limit_editor.set_block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Default Query Limit ")
                                .border_style(limit_style),
                        );
                        f.render_widget(app.gen_limit_editor.widget(), rows[0]);
                    }
                    SettingsTab::Database => {
                        let rows = Layout::default()
                            .constraints([Constraint::Length(3), Constraint::Min(0)])
                            .split(content_area);
                        let timeout_style = if app.settings_focus_index == 0 {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        };
                        app.db_timeout_editor.set_block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Query Timeout (seconds) ")
                                .border_style(timeout_style),
                        );
                        f.render_widget(app.db_timeout_editor.widget(), rows[0]);
                    }
                    SettingsTab::AI => {
                        let rows = Layout::default()
                            .constraints([
                                Constraint::Length(3),
                                Constraint::Length(3),
                                Constraint::Length(3),
                                Constraint::Min(0),
                            ])
                            .split(content_area);
                        let ep_style = if app.settings_focus_index == 0 {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        };
                        app.ai_endpoint_editor.set_block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" AI Endpoint ")
                                .border_style(ep_style),
                        );
                        f.render_widget(app.ai_endpoint_editor.widget(), rows[0]);
                        let model_style = if app.settings_focus_index == 1 {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        };
                        app.ai_model_editor.set_block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" AI Model ")
                                .border_style(model_style),
                        );
                        f.render_widget(app.ai_model_editor.widget(), rows[1]);
                        let key_style = if app.settings_focus_index == 2 {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        };
                        app.ai_key_editor.set_block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" API Key ")
                                .border_style(key_style),
                        );
                        f.render_widget(app.ai_key_editor.widget(), rows[2]);
                    }
                }

                let footer = Paragraph::new(
                    " Tab: next field | [: previous tab | ]: next tab | Enter: save | Esc: normal ",
                )
                .style(Style::default().fg(Color::DarkGray));
                f.render_widget(footer, settings_layout[2]);
            }

            if app.mode == Mode::Help {
                render_help(f, f.size());
            }
        })?;

        if let Some(Event::Key(key)) = events.next().await {
            handle_key(&mut app, key, &settings_ref).await;
        }

        if app.should_quit {
            break;
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn render_login(f: &mut Frame<'_>, area: Rect, app: &mut CryoDbTui<'_>) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);
    let sidebar = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[0]);

    let recents: Vec<ListItem> = app
        .filtered_recents()
        .iter()
        .map(|(label, _)| ListItem::new(format!(" {}", label)))
        .collect();
    let recent_list = List::new(recents)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(login_title(app, LoginPane::Recents, "Recent"))
                .border_style(login_style(app, LoginPane::Recents)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    f.render_stateful_widget(recent_list, sidebar[0], &mut app.recent_state);

    let favorites: Vec<ListItem> = app
        .filtered_favorites()
        .iter()
        .map(|(label, _)| ListItem::new(format!(" {}", label)))
        .collect();
    let favorite_list = List::new(favorites)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(login_title(app, LoginPane::Favorites, "Favorites"))
                .border_style(login_style(app, LoginPane::Favorites)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    f.render_stateful_widget(favorite_list, sidebar[1], &mut app.favorite_state);

    render_login_form(f, layout[1], app);
}

fn render_login_form(f: &mut Frame<'_>, area: Rect, app: &mut CryoDbTui<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(login_title(app, LoginPane::Form, "Connection"))
        .border_style(login_style(app, LoginPane::Form));
    f.render_widget(block, area);

    if app.login_info.driver == crate::DatabaseDriver::Sqlite {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area);
        render_login_driver(f, rows[0], app);
        let path_style = login_field_style(app, 1);
        render_login_input(
            f,
            rows[1],
            &mut app.login_sqlite_path_editor,
            " SQLite File Path ",
            path_style,
        );
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area);
        render_login_driver(f, rows[0], app);

        let host_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(rows[1]);
        let host_style = login_field_style(app, 1);
        let port_style = login_field_style(app, 2);
        render_login_input(
            f,
            host_row[0],
            &mut app.login_host_editor,
            " Host ",
            host_style,
        );
        render_login_input(
            f,
            host_row[1],
            &mut app.login_port_editor,
            " Port ",
            port_style,
        );

        let database_style = login_field_style(app, 3);
        render_login_input(
            f,
            rows[2],
            &mut app.login_database_editor,
            " Database ",
            database_style,
        );

        let auth_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[3]);
        let username_style = login_field_style(app, 4);
        let password_style = login_field_style(app, 5);
        render_login_input(
            f,
            auth_row[0],
            &mut app.login_username_editor,
            " Username ",
            username_style,
        );
        render_login_input(
            f,
            auth_row[1],
            &mut app.login_password_editor,
            " Password ",
            password_style,
        );
    }
}

fn render_login_driver(f: &mut Frame<'_>, area: Rect, app: &CryoDbTui<'_>) {
    let driver = Paragraph::new(format!("< {} >", app.login_info.driver)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Driver ")
            .border_style(login_field_style(app, 0)),
    );
    f.render_widget(driver, area);
}

fn render_login_input(
    f: &mut Frame<'_>,
    area: Rect,
    editor: &mut tui_textarea::TextArea<'_>,
    title: &'static str,
    style: Style,
) {
    editor.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(style),
    );
    f.render_widget(editor.widget(), area);
}

fn render_status(f: &mut Frame<'_>, area: Rect, app: &CryoDbTui<'_>) {
    let status_text = if app.mode == Mode::Command {
        format!(" :{}", app.command_editor.lines().join(""))
    } else if login_screen(app) {
        format!(
            " {} | login:{} | {}",
            app.mode.label(),
            login_pane_label(app.login_pane),
            app.status_message
        )
    } else if app.pool.is_some() {
        format!(
            " {} | {}{} | {} | {} | DB: {}",
            app.mode.label(),
            app.pane.label(),
            if app.window_prefix { " | Ctrl-w" } else { "" },
            app.status_message,
            app.connection.driver,
            app.connection.database
        )
    } else {
        format!(" {} | {}", app.mode.label(), app.status_message)
    };
    let status =
        Paragraph::new(status_text).block(Block::default().borders(Borders::ALL).title(" Status "));
    f.render_widget(status, area);
}

fn login_screen(app: &CryoDbTui<'_>) -> bool {
    app.pool.is_none()
        || matches!(
            app.mode,
            Mode::Prompt(PromptMode::Connection) | Mode::Insert(InsertMode::Login)
        )
}

fn login_style(app: &CryoDbTui<'_>, pane: LoginPane) -> Style {
    if app.login_pane == pane {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn login_title(app: &CryoDbTui<'_>, pane: LoginPane, title: &str) -> String {
    if app.login_pane == pane {
        format!(" {} * ", title)
    } else {
        format!(" {} ", title)
    }
}

fn login_pane_label(pane: LoginPane) -> &'static str {
    match pane {
        LoginPane::Recents => "recent",
        LoginPane::Favorites => "favorites",
        LoginPane::Form => "form",
    }
}

fn login_field_style(app: &CryoDbTui<'_>, field: usize) -> Style {
    if app.login_pane == LoginPane::Form && app.login_field == field {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

async fn handle_key(app: &mut CryoDbTui<'_>, key: KeyEvent, settings: &crate::Settings) {
    match app.mode {
        Mode::Normal => handle_normal_key(app, key).await,
        Mode::Insert(InsertMode::Query) => handle_query_insert_key(app, key),
        Mode::Insert(InsertMode::Login) => handle_login_insert_key(app, key),
        Mode::Command => handle_command_key(app, key, settings).await,
        Mode::Prompt(PromptMode::Ai) => handle_ai_key(app, key).await,
        Mode::Prompt(PromptMode::Database) => handle_database_key(app, key).await,
        Mode::Prompt(PromptMode::Connection) => handle_connection_key(app, key).await,
        Mode::Prompt(PromptMode::Settings) => handle_settings_key(app, key),
        Mode::Prompt(PromptMode::Search) => handle_search_key(app, key),
        Mode::Help => handle_help_key(app, key),
    }
}

async fn handle_normal_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    if app.window_prefix {
        handle_window_key(app, key);
        return;
    }

    if key.code == KeyCode::Char('w') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.window_prefix = true;
        app.status_message = String::from("Ctrl-w: h/j/k/l/w");
        return;
    }

    match key.code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('?') => app.mode = Mode::Help,
        KeyCode::Char(':') => app.start_command(),
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Char('i') => {
            app.focus_pane(Pane::Query);
            app.mode = Mode::Insert(InsertMode::Query);
        }
        KeyCode::Char('h') | KeyCode::Left => handle_pane_motion(app, KeyCode::Left).await,
        KeyCode::Char('j') | KeyCode::Down => handle_pane_motion(app, KeyCode::Down).await,
        KeyCode::Char('k') | KeyCode::Up => handle_pane_motion(app, KeyCode::Up).await,
        KeyCode::Char('l') | KeyCode::Right => handle_pane_motion(app, KeyCode::Right).await,
        KeyCode::PageDown => {
            if app.pane == Pane::View {
                app.scroll_results_down()
            }
        }
        KeyCode::PageUp => {
            if app.pane == Pane::View {
                app.scroll_results_up()
            }
        }
        KeyCode::Enter => match app.pane {
            Pane::Explorer => app.load_selected_table_data().await,
            Pane::View => {}
            Pane::Query => app.run_active_query().await,
        },
        _ => {}
    }
}

async fn handle_pane_motion(app: &mut CryoDbTui<'_>, code: KeyCode) {
    match (app.pane, code) {
        (Pane::Explorer, KeyCode::Down) => app.next_table(),
        (Pane::Explorer, KeyCode::Up) => app.previous_table(),
        (Pane::Explorer, KeyCode::Right) => app.load_selected_table_data().await,
        (Pane::View, KeyCode::Down) => app.scroll_results_down(),
        (Pane::View, KeyCode::Up) => app.scroll_results_up(),
        (Pane::View, KeyCode::Left) => app.scroll_results_left(),
        (Pane::View, KeyCode::Right) => app.scroll_results_right(),
        (Pane::Query, KeyCode::Left | KeyCode::Down | KeyCode::Up | KeyCode::Right) => {
            app.move_query_cursor(code)
        }
        _ => {}
    }
}

fn handle_window_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    app.window_prefix = false;
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => app.move_pane_left(),
        KeyCode::Char('j') | KeyCode::Down => app.move_pane_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_pane_up(),
        KeyCode::Char('l') | KeyCode::Right => app.move_pane_right(),
        KeyCode::Char('w') => match app.pane {
            Pane::Explorer => app.focus_pane(Pane::View),
            Pane::View => app.focus_pane(Pane::Query),
            Pane::Query => app.focus_pane(Pane::Explorer),
        },
        KeyCode::Esc => {}
        _ => app.status_message = String::from("Ctrl-w expects h/j/k/l/w"),
    }
}

fn handle_query_insert_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.mode = Mode::Normal,
        _ => {
            app.pane = Pane::Query;
            app.query_editor.input(key);
        }
    }
}

fn handle_login_insert_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.mode = Mode::Prompt(PromptMode::Connection),
        _ => app.input_login_key(key),
    }
}

async fn handle_command_key(app: &mut CryoDbTui<'_>, key: KeyEvent, settings: &crate::Settings) {
    match key.code {
        KeyCode::Esc => app.mode = Mode::Normal,
        KeyCode::Enter => execute_command(app, settings).await,
        _ => {
            app.command_editor.input(key);
        }
    }
}

async fn execute_command(app: &mut CryoDbTui<'_>, settings: &crate::Settings) {
    let command = app.command_text();
    app.mode = Mode::Normal;
    match command.as_str() {
        "" => {}
        "q" | "quit" => app.quit(),
        "run" | "r" => {
            app.focus_pane(Pane::Query);
            app.run_active_query().await;
        }
        "ai" => app.mode = Mode::Prompt(PromptMode::Ai),
        "db" | "database" => {
            app.mode = Mode::Prompt(PromptMode::Database);
            app.refresh_databases().await;
        }
        "conn" | "connection" | "connections" => app.mode = Mode::Prompt(PromptMode::Connection),
        "set" | "settings" => {
            app.setup_settings_editors(settings);
            app.mode = Mode::Prompt(PromptMode::Settings);
        }
        "help" | "h" | "?" => app.mode = Mode::Help,
        _ => app.status_message = format!("Unknown command: {}", command),
    }
}

async fn handle_ai_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.mode = Mode::Normal,
        KeyCode::Enter => app.generate_with_ai().await,
        _ => {
            app.ai_prompt_editor.input(key);
        }
    }
}

async fn handle_database_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.mode = Mode::Normal,
        KeyCode::Char('?') => app.mode = Mode::Help,
        KeyCode::Char('j') | KeyCode::Down => app.next_db(),
        KeyCode::Char('k') | KeyCode::Up => app.previous_db(),
        KeyCode::Enter => app.use_selected_db().await,
        _ => {}
    }
}

async fn handle_connection_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    if app.window_prefix {
        handle_login_window_key(app, key);
        return;
    }

    if key.code == KeyCode::Char('w') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.window_prefix = true;
        app.status_message = String::from("Ctrl-w: h/j/k/l/w");
        return;
    }

    match key.code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('?') => app.mode = Mode::Help,
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Esc => {
            if app.pool.is_some() {
                app.mode = Mode::Normal
            }
        }
        KeyCode::Char('i') if app.login_pane == LoginPane::Form => {
            app.mode = Mode::Insert(InsertMode::Login)
        }
        KeyCode::Char('[') if app.login_pane == LoginPane::Form => app.previous_login_driver(),
        KeyCode::Char(']') if app.login_pane == LoginPane::Form => app.next_login_driver(),
        KeyCode::Char('j') | KeyCode::Down => app.next_connection(),
        KeyCode::Char('k') | KeyCode::Up => app.previous_connection(),
        KeyCode::Enter => {
            if app.login_pane == LoginPane::Form {
                app.connect_login_form().await;
            } else {
                app.fill_login_form_from_selection();
            }
        }
        _ => {}
    }
}

fn handle_search_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.search_editor = TextArea::default();
            app.mode = if login_screen(app) {
                Mode::Prompt(PromptMode::Connection)
            } else {
                Mode::Normal
            };
        }
        KeyCode::Enter => {
            app.mode = if login_screen(app) {
                Mode::Prompt(PromptMode::Connection)
            } else {
                Mode::Normal
            };
        }
        _ => {
            app.search_editor.input(key);
        }
    }
}

fn handle_login_window_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    app.window_prefix = false;
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => app.focus_login_sidebar(),
        KeyCode::Char('l') | KeyCode::Right => app.focus_login_form(),
        KeyCode::Char('j') | KeyCode::Down => match app.login_pane {
            LoginPane::Recents => {
                app.login_pane = if app.favorite_connections.is_empty() {
                    LoginPane::Form
                } else {
                    LoginPane::Favorites
                }
            }
            LoginPane::Favorites => app.login_pane = LoginPane::Form,
            LoginPane::Form => {}
        },
        KeyCode::Char('k') | KeyCode::Up => match app.login_pane {
            LoginPane::Recents => {}
            LoginPane::Favorites => {
                app.login_pane = if app.recent_connections.is_empty() {
                    LoginPane::Form
                } else {
                    LoginPane::Recents
                }
            }
            LoginPane::Form => {
                app.login_pane = if !app.favorite_connections.is_empty() {
                    LoginPane::Favorites
                } else if !app.recent_connections.is_empty() {
                    LoginPane::Recents
                } else {
                    LoginPane::Form
                }
            }
        },
        KeyCode::Char('w') => {
            app.login_pane = match app.login_pane {
                LoginPane::Recents => {
                    if app.favorite_connections.is_empty() {
                        LoginPane::Form
                    } else {
                        LoginPane::Favorites
                    }
                }
                LoginPane::Favorites => LoginPane::Form,
                LoginPane::Form => {
                    if !app.recent_connections.is_empty() {
                        LoginPane::Recents
                    } else if !app.favorite_connections.is_empty() {
                        LoginPane::Favorites
                    } else {
                        LoginPane::Form
                    }
                }
            }
        }
        KeyCode::Esc => {}
        _ => app.status_message = String::from("Ctrl-w expects h/j/k/l/w"),
    }
    if app.login_pane != LoginPane::Form {
        app.preview_login_selection();
    }
}

fn handle_settings_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.mode = Mode::Normal,
        KeyCode::Char('[') => {
            app.settings_tab = match app.settings_tab {
                SettingsTab::General => SettingsTab::AI,
                SettingsTab::Database => SettingsTab::General,
                SettingsTab::AI => SettingsTab::Database,
            };
            app.settings_focus_index = 0;
        }
        KeyCode::Char(']') => {
            app.settings_tab = match app.settings_tab {
                SettingsTab::General => SettingsTab::Database,
                SettingsTab::Database => SettingsTab::AI,
                SettingsTab::AI => SettingsTab::General,
            };
            app.settings_focus_index = 0;
        }
        KeyCode::Tab => {
            app.settings_focus_index =
                (app.settings_focus_index + 1) % settings_field_count(app.settings_tab)
        }
        KeyCode::BackTab => {
            let count = settings_field_count(app.settings_tab);
            app.settings_focus_index = if app.settings_focus_index == 0 {
                count - 1
            } else {
                app.settings_focus_index - 1
            };
        }
        KeyCode::Enter => app.save_settings(),
        _ => match app.settings_tab {
            SettingsTab::General => {
                app.gen_limit_editor.input(key);
            }
            SettingsTab::Database => {
                app.db_timeout_editor.input(key);
            }
            SettingsTab::AI => match app.settings_focus_index {
                0 => {
                    app.ai_endpoint_editor.input(key);
                }
                1 => {
                    app.ai_model_editor.input(key);
                }
                2 => {
                    app.ai_key_editor.input(key);
                }
                _ => {}
            },
        },
    }
}

fn handle_help_key(app: &mut CryoDbTui<'_>, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.mode = if app.pool.is_none() {
                Mode::Prompt(PromptMode::Connection)
            } else {
                Mode::Normal
            };
        }
        _ => {}
    }
}

fn settings_field_count(tab: SettingsTab) -> usize {
    match tab {
        SettingsTab::General => 1,
        SettingsTab::Database => 1,
        SettingsTab::AI => 3,
    }
}

fn explorer_shows_connections(app: &CryoDbTui<'_>) -> bool {
    app.pool.is_none() || app.mode == Mode::Prompt(PromptMode::Connection)
}

fn pane_style(app: &CryoDbTui<'_>, pane: Pane) -> Style {
    if app.pane == pane {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn pane_title(app: &CryoDbTui<'_>, pane: Pane, title: &str) -> String {
    if app.pane == pane {
        format!(" {} * ", title)
    } else {
        format!(" {} ", title)
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn recent_items(store: &ConnectionStore) -> Vec<(String, StoredConnection)> {
    store
        .recents
        .iter()
        .map(|entry| (entry.profile_label(), entry.clone()))
        .collect()
}

fn favorite_items(store: &ConnectionStore) -> Vec<(String, StoredConnection)> {
    let mut items = Vec::new();
    for entry in &store.favorites {
        items.push((entry.profile_label(), entry.clone()));
    }
    for entry in &store.profiles {
        items.push((format!("Profile: {}", entry.profile_label()), entry.clone()));
    }
    items
}

fn render_help(f: &mut Frame, area: Rect) {
    let section_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let sections: Vec<(&str, Vec<[&str; 2]>)> = vec![
        (
            "Navigation",
            vec![
                ["h/j/k/l / arrows", "Move within pane"],
                ["Ctrl-w h/j/k/l", "Move focus to pane"],
                ["Ctrl-w w", "Cycle pane focus"],
            ],
        ),
        (
            "Normal Mode",
            vec![
                ["Enter", "Open table / Run query"],
                ["i", "Edit query / field"],
                ["Esc", "Cancel / leave mode"],
                ["/ / :", "Filter / Command prompt"],
                ["?", "Toggle help"],
                ["q", "Quit"],
            ],
        ),
        (
            "Login Screen",
            vec![
                ["j/k", "Move between items"],
                ["Ctrl-w h/l", "Sidebar ↔ Form"],
                ["Ctrl-w j/k", "Recents ↔ Favs ↔ Form"],
                ["Enter", "Fill form / Connect"],
                ["i / [ / ]", "Edit field / Switch driver"],
            ],
        ),
        (
            "Commands (:)",
            vec![
                [":run / :r", "Run query"],
                [":ai", "AI prompt"],
                [":db / :use", "Switch database"],
                [":conn", "Connection picker"],
                [":settings / :help", "Settings / Help"],
                [":q", "Quit"],
            ],
        ),
        (
            "Settings",
            vec![
                ["Tab / S-Tab", "Move field"],
                ["[ / ] / Enter", "Switch tab / Save"],
                ["Esc", "Close"],
            ],
        ),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help (Esc/q/?: close) ");
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let total_rows: usize = sections.iter().map(|(_, r)| r.len()).sum();
    let constraints: Vec<Constraint> = sections
        .iter()
        .map(|(_, r)| Constraint::Ratio(r.len() as u32, total_rows as u32))
        .collect();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, (title, rows)) in sections.iter().enumerate() {
        let table_rows: Vec<Row> = rows.iter().map(|[k, d]| Row::new(vec![*k, *d])).collect();
        let section_block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::TOP)
            .border_style(section_style);
        let table = Table::new(table_rows, &[Constraint::Length(20), Constraint::Min(20)])
            .block(section_block)
            .column_spacing(2);
        f.render_widget(table, chunks[i]);
    }
}

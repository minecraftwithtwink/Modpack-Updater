use ratatui::widgets::ListState;
use crate::app::{App, AppState, RunMode, TutorialState};
use crate::music::MusicPlayer;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{block::Title, Block, Borders, Clear, Gauge, List, ListItem, Paragraph},
    Frame,
};

const MINECRAFT_VERSION: &str = "1.21.1";
const NEOFORGE_VERSION: &str = "21.1.175";

pub fn draw(f: &mut Frame, app: &mut App, music_player: &MusicPlayer) {
    let is_dimmed = !matches!(app.state, AppState::Browsing)
        || (app.tutorial.is_some() && !app.tutorial_interactive && !app.tutorial_paused);

    match app.mode {
        RunMode::StartupSelection => draw_startup_ui(f, app, music_player, is_dimmed),
        RunMode::FileBrowser => draw_browsing_ui(f, app, music_player, is_dimmed),
    }

    if app.tutorial.is_some() && !app.tutorial_paused {
        draw_tutorial_popup(f, app);
    } else {
        match &mut app.state {
            AppState::AwaitingInput => draw_input_ui(f, app),
            AppState::ConfirmReinit => draw_confirm_ui(f),
            AppState::ConfirmUpdate { version } => draw_confirm_update_popup(f, version),
            AppState::FetchingChangelog => draw_fetching_popup(f, "Fetching Changelog..."),
            AppState::ViewingChangelog { content, scroll } => draw_changelog_popup(f, content, *scroll),
            // --- ADDED: Call the new branch selection drawers ---
            AppState::FetchingBranches => draw_fetching_popup(f, "Fetching Branches..."),
            AppState::BranchSelection { branches, list_state, selected_branch } => {
                draw_branch_selection_popup(f, branches, list_state, selected_branch);
            }
            AppState::Processing { message, progress } => draw_processing_ui(f, message, *progress),
            AppState::Finished(msg) => draw_finished_ui(f, msg),
            AppState::ConfirmInvalidFolder { path } => draw_invalid_folder_popup(f, &path.display().to_string()),
            AppState::InsideInstanceFolderError => draw_inside_folder_error_popup(f),
            _ => {}
        }
    }
}

// --- ADDED: The new popup for selecting a branch ---
fn draw_branch_selection_popup(
    f: &mut Frame,
    branches: &[String],
    list_state: &mut ListState,
    selected_branch: &Option<String>,
) {
    let popup_width = 60;
    let popup_height = 15;
    let area = centered_rect(popup_width, popup_height, f.size());

    let items: Vec<ListItem> = branches
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let is_hovered = Some(i) == list_state.selected();
            let is_selected = Some(name) == selected_branch.as_ref();

            let style = if is_selected && is_hovered {
                Style::default().bg(Color::Green).fg(Color::Black)
            } else if is_selected {
                Style::default().fg(Color::Green)
            } else if is_hovered {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            let mut line = name.clone();
            if is_hovered && is_selected {
                line.push_str(" (confirm?)");
            }
            ListItem::new(Span::styled(line, style))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Select a Branch "))
        .highlight_symbol("> ");

    f.render_widget(Clear, area);
    f.render_stateful_widget(list, area, list_state);
}


fn draw_fetching_popup(f: &mut Frame, message: &str) {
    let text = Text::from(vec![
        Line::from(""),
        Line::from(message),
        Line::from(""),
    ]);
    let block = Block::default().title(" Please Wait ").borders(Borders::ALL);
    let text_widget = Paragraph::new(text).block(block).alignment(Alignment::Center);
    let area = centered_rect(40, 5, f.size());
    f.render_widget(Clear, area);
    f.render_widget(text_widget, area);
}

fn draw_changelog_popup(f: &mut Frame, content: &str, scroll: u16) {
    let text = Text::from(content);

    let popup_width = (f.size().width as f32 * 0.8) as u16;
    let popup_height = (f.size().height as f32 * 0.8) as u16;
    let area = centered_rect(popup_width, popup_height, f.size());

    let block = Block::default()
        .title(" Changelog (↑/↓ to scroll, Esc to close) ")
        .borders(Borders::ALL);

    let paragraph = Paragraph::new(text)
        .block(block)
        .scroll((scroll, 0));

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}


fn draw_confirm_update_popup(f: &mut Frame, version: &str) {
    let green_style = Style::default().fg(Color::Green);
    let key_style = Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD);

    let text = Text::from(vec![
        Line::from(vec![
            Span::raw("A new version ("),
            Span::styled(version, green_style.add_modifier(Modifier::BOLD)),
            Span::raw(") is available!"),
        ]),
        Line::from(""),
        Line::from("Would you like to update now?"),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Y ", Style::default().fg(Color::Black).bg(Color::Green)),
            Span::raw(" Yes "),
            Span::styled(" Esc ", key_style),
            Span::raw(" No (update on next launch) "),
        ]),
    ]);

    let popup_width = (text.width() + 4).min(f.size().width.into());
    let popup_height = (text.height() as u16 + 2).min(f.size().height);
    let area = centered_rect(popup_width.try_into().unwrap(), popup_height, f.size());

    let block = Block::default()
        .title(" Update Available ")
        .borders(Borders::ALL)
        .border_style(green_style);
    let text_widget = Paragraph::new(text).block(block).alignment(Alignment::Center);

    f.render_widget(Clear, area);
    f.render_widget(text_widget, area);
}


fn draw_tutorial_popup(f: &mut Frame, app: &mut App) {
    let tutorial_state = app.tutorial.unwrap();
    let gold_style = Style::default().fg(Color::Yellow);
    let key_style = Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD);
    let cyan_style = Style::default().fg(Color::Cyan);
    let green_style = Style::default().fg(Color::Green);
    let red_style = Style::default().fg(Color::Red);
    let yellow_key_style = Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD);

    let (title, text) = match tutorial_state {
        TutorialState::Welcome => (
            " Welcome to the Modpack Updater! ",
            Text::from(vec![
                Line::from(vec![
                    Span::raw("This tool is programmed by "),
                    Span::styled(
                        " @Metalhead Twink★ ",
                        Style::default().fg(Color::LightMagenta).bg(Color::Black).add_modifier(Modifier::BOLD).add_modifier(Modifier::ITALIC),
                    ),
                    Span::raw(" to help you keep"),
                ]),
                Line::from("your Minecraft modpack instance up-to-date with the official Git repository."),
                Line::from(""),
                Line::from("Let's walk through the steps together."),
                Line::from(""),
                Line::from(vec![ Span::styled( "Press any key to continue...", Style::default().add_modifier(Modifier::ITALIC), ), ]),
            ]),
        ),
        TutorialState::StartupMenu => {
            let mut lines = vec![
                Line::from("This is the instance selection menu."),
                Line::from("Since it's your first time, the list is empty."),
                Line::from(""),
                Line::from(vec![ Span::raw("Press "), Span::styled(" ↓ ", key_style), Span::raw(" to select '"), Span::styled("Specify a new Instance...", cyan_style), Span::raw("'."), ]),
                Line::from(vec![ Span::raw("Then press "), Span::styled(" Enter ", key_style), Span::raw(" to continue."), ]),
                Line::from(""),
                Line::from(vec![ Span::styled("Press 'H' for a hint", Style::default().add_modifier(Modifier::ITALIC)) ])
            ];

            if app.tutorial_step1_expanded {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled("What is an Instance?", Style::default().add_modifier(Modifier::BOLD))));
                lines.push(Line::from("An instance is a dedicated folder for a specific, modded"));
                lines.push(Line::from("version of Minecraft. Launchers like Modrinth, CurseForge,"));
                lines.push(Line::from("or SKLauncher create these for you."));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![ Span::raw("Please create a custom instance for Minecraft "), Span::styled(MINECRAFT_VERSION, gold_style), ]));
                lines.push(Line::from(vec![ Span::raw("using NeoForge "), Span::styled(NEOFORGE_VERSION, gold_style), Span::raw(" before proceeding."), ]));
            }
            (" Step 1: Select an Instance ", Text::from(lines))
        },
        TutorialState::FileBrowserNav => (
            " Step 2: Find Your Instance Folder ",
            Text::from(vec![
                Line::from("Now, navigate to your Minecraft instance folder."),
                Line::from(vec![ Span::styled(" ↑ ", key_style), Span::raw("       "), Span::styled(" ↑ ", Style::default().fg(Color::Black).bg(Color::DarkGray).add_modifier(Modifier::BOLD)), Span::raw("                                     "), ]),
                Line::from(vec![ Span::raw("Use "), Span::styled(" ← ", Style::default().fg(Color::Black).bg(Color::DarkGray).add_modifier(Modifier::BOLD)), Span::styled(" ↓ ", key_style), Span::styled(" → ", Style::default().fg(Color::Black).bg(Color::DarkGray).add_modifier(Modifier::BOLD)), Span::raw(" "), Span::styled(" ← ", key_style), Span::styled(" ↓ ", Style::default().fg(Color::Black).bg(Color::DarkGray).add_modifier(Modifier::BOLD)), Span::styled(" → ", key_style), Span::raw(" or use "), Span::styled(" Ctrl+F ", yellow_key_style), Span::raw(" to enter a path directly."), ]),
                Line::from(""),
                Line::from(vec![ Span::raw("When you are in the correct directory, press "), Span::styled(" Enter ", key_style)]),
                Line::from("to proceed to the selection step."),
            ])
        ),
        TutorialState::InsideInstanceFolderHint => (
            " Whoops! A Quick Tip ",
            Text::from(vec![ Line::from("It looks like you're currently *inside* the instance folder."), Line::from("You need to select the folder that *contains* `mods`, `config`, etc."), Line::from(""), Line::from(vec![Span::raw("Please press "), Span::styled(" ← ", key_style), Span::raw(" to go up one level.")]), ])
        ),
        TutorialState::FileBrowserSelect => ( " Step 3: Select the Folder ", Text::from(vec![ Line::from("The folder list is now active for selection."), Line::from(""), Line::from(vec![ Span::raw("Press "), Span::styled(" Enter ", key_style), Span::raw(" while hovering on a folder to select it."), ]), Line::from(""), Line::from(vec![ Span::styled("Hint: ", green_style), Span::raw("You can use "), Span::styled(" Escape ", key_style), Span::raw(" to go back or undo selection."), ]), Line::from(""), Line::from(vec![ Span::raw("The selected folder will turn "), Span::styled("green", green_style), Span::raw("."), ]), ]) ),
        TutorialState::InvalidSelectionHint => (
            " Invalid Folder ",
            Text::from(vec![
                Line::from(Span::styled("That folder doesn't look like a valid instance.", red_style)),
                Line::from(""),
                Line::from("Please select the main instance folder that contains"),
                Line::from(Span::raw("the `mods` and `config` subfolders.")),
                Line::from(""),
                Line::from(vec![Span::raw("Press "), Span::styled(" Enter ", key_style), Span::raw(" to try again.")])
            ])
        ),
        TutorialState::FileBrowserConfirm => ( " Step 4: Confirm Your Selection ", Text::from(vec![ Line::from("Great! The folder is now selected."), Line::from(""), Line::from(vec![ Span::raw("Press "), Span::styled(" Enter ", key_style), Span::raw(" one more time to confirm and start the update."), ]), Line::from(""), Line::from("This is the final step of the tutorial!"), ]) ),
    };

    let popup_width = (text.width() as u16 + 4).min(f.size().width);
    let popup_height = (text.height() as u16 + 2).min(f.size().height);

    let block = Block::default()
        .title(Span::styled(title, gold_style))
        .borders(Borders::ALL)
        .border_style(gold_style);
    let text_widget = Paragraph::new(text).block(block).alignment(Alignment::Center);
    let area = centered_rect(popup_width, popup_height, f.size());
    if tutorial_state == TutorialState::Welcome { f.render_widget(Clear, f.size()); }
    f.render_widget(Clear, area);
    f.render_widget(text_widget, area);
}

fn draw_inside_folder_error_popup(f: &mut Frame) {
    let red_style = Style::default().fg(Color::Red);
    let key_style = Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD);

    let text = Text::from(vec![
        Line::from(Span::styled("You are currently inside an instance folder.", red_style)),
        Line::from("You need to select the folder that *contains* `mods`, `config`, etc."),
        Line::from(""),
        Line::from(vec![
            Span::raw("Please press "),
            Span::styled(" ← ", key_style),
            Span::raw(" to go up to the parent directory."),
        ]),
    ]);

    let popup_width = (text.width() + 4).min(f.size().width.into());
    let popup_height = (text.height() as u16 + 2).min(f.size().height);
    let area = centered_rect(popup_width.try_into().unwrap(), popup_height, f.size());

    let block = Block::default()
        .title(" Incorrect Directory ")
        .borders(Borders::ALL)
        .border_style(red_style);
    let text_widget = Paragraph::new(text).block(block).alignment(Alignment::Center);

    f.render_widget(Clear, area);
    f.render_widget(text_widget, area);
}

fn draw_invalid_folder_popup(f: &mut Frame, path_str: &str) {
    let red_style = Style::default().fg(Color::Red);
    let key_style = Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD);

    let text = Text::from(vec![
        Line::from(vec![ Span::styled("The folder '", red_style), Span::styled(path_str, red_style.add_modifier(Modifier::BOLD)), Span::styled("' does not look like a", red_style) ]),
        Line::from(Span::styled("valid Minecraft instance.", red_style)),
        Line::from(""),
        Line::from("A valid instance folder should directly contain"),
        Line::from("subfolders like `mods` and `config`."),
        Line::from(""),
        Line::from(vec![ Span::raw("Press "), Span::styled(" Enter ", key_style), Span::raw(" or "), Span::styled(" Esc ", key_style), Span::raw(" to return and select a different folder."), ]),
    ]);

    let popup_width = (text.width() + 4).min(f.size().width.into());
    let popup_height = (text.height() as u16 + 2).min(f.size().height);
    let area = centered_rect(popup_width.try_into().unwrap(), popup_height, f.size());

    let block = Block::default()
        .title(" Invalid Folder Selected ")
        .borders(Borders::ALL)
        .border_style(red_style);
    let text_widget = Paragraph::new(text).block(block).alignment(Alignment::Center);

    f.render_widget(Clear, area);
    f.render_widget(text_widget, area);
}

fn draw_music_bar(f: &mut Frame, area: Rect, music_player: &MusicPlayer, is_dimmed: bool) {
    let (title, artist, song_style) = music_player.get_current_song_info();
    let dimmed_style = Style::default().fg(Color::DarkGray);
    let final_song_style = if is_dimmed { dimmed_style } else { song_style };
    let final_artist_style = if is_dimmed { dimmed_style } else { Style::default() };
    let final_label_style = if is_dimmed { dimmed_style } else { Style::default().add_modifier(Modifier::BOLD) };
    let status_text = if music_player.is_paused { "(Paused)" } else { "(Playing)" };
    let padded_status_text = format!("{:^9}", status_text);
    let status_style = if is_dimmed {
        dimmed_style
    } else if music_player.is_paused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::LightCyan)
    };
    let music_text = Line::from(vec![
        Span::styled("Current Track: ", final_label_style),
        Span::styled(title, final_song_style),
        Span::styled(format!(" - {} ", artist), final_artist_style),
        Span::styled(padded_status_text, status_style),
    ]);
    let music_line_widget = Paragraph::new(music_text).alignment(Alignment::Center);
    f.render_widget(music_line_widget, area);
}

fn draw_startup_ui(f: &mut Frame, app: &mut App, music_player: &MusicPlayer, is_dimmed: bool) {
    let size = f.size();
    let layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(2)]).split(size);
    let header_style = if is_dimmed { Style::default().fg(Color::DarkGray) } else { Style::default() };

    let mut items: Vec<ListItem> = app.history.iter().map(|p| {
        ListItem::new(Span::styled(p.display().to_string(), header_style))
    }).collect();

    let new_instance_style = if is_dimmed { header_style } else { Style::default().fg(Color::Cyan) };
    items.push(ListItem::new(Span::styled("Specify a new Instance...", new_instance_style)));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Select an Instance to Update ").style(header_style))
        .highlight_style(if is_dimmed { header_style } else { Style::default().add_modifier(Modifier::REVERSED) })
        .highlight_symbol(if is_dimmed { " " } else { "> " });

    f.render_stateful_widget(list, layout[0], &mut app.history_state);

    draw_music_bar(f, layout[1], music_player, is_dimmed);

    const MUSIC_TOOLTIP_WIDTH: usize = 13;
    let music_text = if music_player.is_paused { "Play Music  " } else { "Pause Music  " };
    let music_status_tooltip = format!(" {:<width$} ", music_text, width = MUSIC_TOOLTIP_WIDTH);

    // --- MODIFIED: Added 'C' for Changelog to the footer ---
    let footer_lines = vec![
        Line::from(vec![Span::raw("      "), Span::styled(" ↑ ", if is_dimmed {header_style} else {Style::default().bg(Color::Blue).fg(Color::White)})]),
        Line::from(vec![
            Span::raw("   "),
            Span::styled(" ← ", Style::default().bg(Color::DarkGray).fg(Color::Black)), Span::styled(" ↓ ", if is_dimmed {header_style} else {Style::default().bg(Color::Blue).fg(Color::White)}), Span::styled(" → ", Style::default().bg(Color::DarkGray).fg(Color::Black)), Span::raw(" Scroll Up/Down   "),
            Span::styled(" Enter ", if is_dimmed {header_style} else {Style::default().bg(Color::Green).fg(Color::White)}), Span::raw(" Confirm   "),
            Span::styled(" C ", if is_dimmed {header_style} else {Style::default().bg(Color::Yellow).fg(Color::Black)}), Span::raw(" Changelog   "),
            Span::styled(" P ", if is_dimmed {header_style} else {Style::default().bg(Color::Cyan).fg(Color::White)}), Span::raw(&music_status_tooltip),
            Span::styled(" Q/Esc ", if is_dimmed {header_style} else {Style::default().bg(Color::Red).fg(Color::White)}), Span::raw(" Quit   "),
        ]),
    ];
    f.render_widget(Paragraph::new(footer_lines).style(header_style), layout[2]);
}

fn draw_browsing_ui(f: &mut Frame, app: &mut App, music_player: &MusicPlayer, is_dimmed: bool) {
    let size = f.size();
    let layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(4), Constraint::Min(1), Constraint::Length(1), Constraint::Length(2)]).split(size);
    let header_style = if is_dimmed { Style::default().fg(Color::DarkGray) } else { Style::default() };
    let dimmed_bg_style = if is_dimmed { Style::default().fg(Color::Black).bg(Color::DarkGray) } else { Style::default() };
    let selected_style = if is_dimmed { header_style } else { Style::default().fg(Color::Green) };
    let mut header_lines = vec![Line::from(vec![ Span::styled(" Current path: ", header_style.add_modifier(Modifier::BOLD)), Span::styled(app.current_dir.display().to_string(), header_style) ])];
    if let Some(default) = &app.confirmed_path {
        header_lines.push(Line::from(vec![Span::styled(" Confirmed: ", header_style.add_modifier(Modifier::BOLD)), Span::styled(default.display().to_string(), header_style)]));
    } else if let Some(selected) = &app.selected_path {
        header_lines.push(Line::from(vec![Span::styled(" Selected: ", selected_style.add_modifier(Modifier::BOLD)), Span::styled(selected.display().to_string(), selected_style)]));
    }

    let version = env!("CARGO_PKG_VERSION");
    let authors = env!("CARGO_PKG_AUTHORS");
    let credit_text = format!(" Modpack Updater v{} | by @{} ", version, authors);
    let info_block = Block::default()
        .borders(Borders::ALL)
        .style(header_style)
        .title(Title::from(" Info ").alignment(Alignment::Left))
        .title(
            Title::from(Span::styled(credit_text, header_style))
                .alignment(Alignment::Right)
        );

    f.render_widget(Paragraph::new(header_lines).block(info_block), layout[0]);

    let list_width = size.width.saturating_sub(2);
    let items: Vec<ListItem> = app.items.iter().enumerate().map(|(i, p)| {
        let filename = p.file_name().unwrap().to_string_lossy();
        let is_hovered = Some(i) == app.list_state.selected();
        let is_selected = Some(p) == app.selected_path.as_ref();
        let style = if is_dimmed { Style::default().fg(Color::DarkGray) } else if is_selected && is_hovered { Style::default().bg(Color::Green).fg(Color::Black) } else if is_selected { Style::default().fg(Color::Green) } else if is_hovered { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
        let mut line = filename.to_string();
        if is_hovered && is_selected && !is_dimmed { line.push_str(" (confirm?)"); }
        if line.len() < list_width as usize { line.push_str(&" ".repeat(list_width as usize - line.len())); }
        ListItem::new(Span::styled(line, style))
    }).collect();
    f.render_stateful_widget(List::new(items).block(Block::default().borders(Borders::ALL).title(" Folders ").style(header_style)).highlight_symbol(if is_dimmed {""} else {"> "}), layout[1], &mut app.list_state);
    draw_music_bar(f, layout[2], music_player, is_dimmed);
    const SELECT_CONTENT_WIDTH: usize = 9;
    const ESC_CONTENT_WIDTH: usize = 10;
    const MUSIC_TOOLTIP_WIDTH: usize = 13;
    let select_text = if app.selected_path.is_some() { "Confirm  " } else { "Select  " };
    let esc_text = if app.selected_path.is_some() { "Deselect  " } else { "Back  " };
    let music_status_tooltip = if music_player.is_paused { "Play Music  " } else { "Pause Music  " };
    let select_status_text = format!(" {:<width$} ", select_text, width = SELECT_CONTENT_WIDTH);
    let esc_status_text = format!(" {:<width$} ", esc_text, width = ESC_CONTENT_WIDTH);
    let music_status_tooltip_padded = format!(" {:<width$} ", music_status_tooltip, width = MUSIC_TOOLTIP_WIDTH);
    let footer_lines = vec![
        Line::from(vec![ Span::raw("      "), Span::styled(" ↑ ", if is_dimmed {header_style} else {Style::default().bg(Color::DarkGray).fg(Color::Black)}), Span::styled("                                ", header_style), Span::styled(" ↑ ", if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Blue).fg(Color::White)})]),
        Line::from(vec![
            Span::raw("   "), Span::styled(" ← ", if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Blue).fg(Color::White)}), Span::styled(" ↓ ", if is_dimmed {header_style} else {Style::default().bg(Color::DarkGray).fg(Color::Black)}), Span::styled(" → ", if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Blue).fg(Color::White)}), Span::styled(" Navigate in/out folder   ", header_style),
            Span::styled(" ← ", if is_dimmed {header_style} else {Style::default().bg(Color::DarkGray).fg(Color::Black)}), Span::styled(" ↓ ", if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Blue).fg(Color::White)}), Span::styled(" → ", if is_dimmed {header_style} else {Style::default().bg(Color::DarkGray).fg(Color::Black)}), Span::styled(" Scroll Up/Down   ", header_style),
            Span::styled(" Enter ", if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Green).fg(Color::White)}), Span::styled(&select_status_text, header_style),
            Span::styled(" Esc ", if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Blue).fg(Color::White)}), Span::styled(&esc_status_text, header_style),
            Span::styled(" Home ",if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Cyan).fg(Color::White)}), Span::styled(" Reset   ", header_style),
            Span::styled(" Ctrl+F ", if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Yellow).fg(Color::White)}), Span::styled(" Change Path   ", header_style),
            Span::styled(" P ", if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Cyan).fg(Color::White)}), Span::styled(&music_status_tooltip_padded, header_style),
            Span::styled(" Q ", if is_dimmed {dimmed_bg_style} else {Style::default().bg(Color::Red).fg(Color::White)}), Span::styled(" Quit", header_style),
        ]),
    ];
    f.render_widget(Paragraph::new(footer_lines), layout[3]);
}

fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - r.height.min(height) * 100 / r.height) / 2),
            Constraint::Length(height),
            Constraint::Percentage((100 - r.height.min(height) * 100 / r.height) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - r.width.min(width) * 100 / r.width) / 2),
            Constraint::Length(width),
            Constraint::Percentage((100 - r.width.min(width) * 100 / r.width) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_input_ui(f: &mut Frame, app: &App) {
    let popup_width = 80; // percent
    let popup_height = if app.input_error.is_some() { 5 } else { 3 };
    let area = centered_rect(f.size().width * popup_width / 100, popup_height, f.size());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Change Directory (Enter to confirm, Esc to cancel) ")
        .borders(Borders::ALL);
    if let Some(err) = &app.input_error {
        let input_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(1)])
            .split(area);
        let input_widget = Paragraph::new(app.input.value()).block(block);
        f.render_widget(input_widget, input_chunks[0]);
        let error_text = Paragraph::new(Span::styled(err, Style::default().fg(Color::Red)))
            .alignment(Alignment::Center);
        f.render_widget(error_text, input_chunks[1]);
    } else {
        let input_widget = Paragraph::new(app.input.value()).block(block);
        f.render_widget(input_widget, area);
    }
    f.set_cursor(
        area.x + app.input.visual_cursor() as u16 + 1,
        area.y + 1,
    );
}

fn draw_processing_ui(f: &mut Frame, message: &str, progress: f64) {
    let popup_width = 60; // percent
    let popup_height = 5;
    let area = centered_rect(f.size().width * popup_width / 100, popup_height, f.size());
    f.render_widget(Clear, area);
    let block = Block::default().title(" Git Operation ").borders(Borders::ALL);
    f.render_widget(block, area);
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1)
        ])
        .split(area);
    let text_widget = Paragraph::new(Text::from(message)).alignment(Alignment::Center);
    f.render_widget(text_widget, inner_chunks[0]);
    let gauge = Gauge::default()
        .ratio(progress)
        .label(format!("{:.0}%", progress * 100.0))
        .style(Style::default().fg(Color::Cyan))
        .gauge_style(Style::default().fg(Color::White).bg(Color::Black).add_modifier(Modifier::BOLD));
    f.render_widget(gauge, inner_chunks[2]);
}

fn draw_confirm_ui(f: &mut Frame) {
    let text = Text::from(vec![
        Line::from("A .git folder will be created or updated."),
        Line::from(""),
        Line::from("This will fetch and merge changes from the remote,"),
        Line::from("updating files tracked by the repository."),
        Line::from("Untracked files will not be affected."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Continue? ", Style::default()),
            Span::styled(" Y ", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" Yes "),
            Span::styled(" N ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(" No "),
        ]),
    ]);
    let popup_width = (text.width() + 4).min(f.size().width.into());
    let popup_height = (text.height() as u16 + 2).min(f.size().height);
    let area = centered_rect(popup_width.try_into().unwrap(), popup_height, f.size());
    f.render_widget(Clear, area);
    let block = Block::default().title(" Confirmation Required ").borders(Borders::ALL);
    let text_widget = Paragraph::new(text).block(block).alignment(Alignment::Center);
    f.render_widget(text_widget, area);
}

fn draw_finished_ui(f: &mut Frame, message: &str) {
    let text = Text::from(message);
    let popup_width = (text.width() + 4).min(f.size().width.into());
    let popup_height = (text.height() as u16 + 2).min(f.size().height);
    let area = centered_rect(popup_width.try_into().unwrap(), popup_height, f.size());
    f.render_widget(Clear, area);
    let block = Block::default().title(" Finished ").borders(Borders::ALL);
    let text_widget = Paragraph::new(text).block(block).alignment(Alignment::Center);
    f.render_widget(text_widget, area);
}
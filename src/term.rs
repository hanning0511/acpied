use anyhow::Result;
use chrono::{Datelike, Timelike, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use file_diff;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{exit, Command};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Spans,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use tui_textarea::{CursorMove, Input, Key, Scrolling, TextArea};

const MAX_HISTORY_SIZE: usize = 100;
const ORIGINDIR: &str = "/tmp/acpidump/origin";
const MODIFIEDDIR: &str = "/tmp/acpidump/modified";
const LOGFILE: &str = "/var/log/acpied.log";

struct StatefulList<T> {
    state: ListState,
    items: Vec<T>,
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
                    0
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
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

enum Mode {
    Normal,
    Insert,
    Search,
}

struct AcpiEditor<'a> {
    files: StatefulList<String>,
    modified: StatefulList<String>,
    content: TextArea<'a>,
    last_char: char,
    mode: Mode,
    search_pattern: TextArea<'a>,
    log: TextArea<'a>,
}

impl AcpiEditor<'_> {
    fn new() -> Self {
        let script = PathBuf::from("/bin/acpied-init");

        let output = Command::new("bash")
            .arg(&script)
            .output()
            .expect("fail to execute script!");
        if !output.status.success() {
            panic!("script executed with error code!");
        }

        let mut files: Vec<String> = vec![];

        for entry in fs::read_dir(MODIFIEDDIR).unwrap() {
            let p = entry.unwrap().path();
            let file_name = String::from(p.file_name().unwrap().to_str().unwrap());
            files.push(file_name);
        }

        files.sort();

        // let modified: Vec<String> = vec![];

        let mut editor = Self {
            files: StatefulList::with_items(files),
            modified: StatefulList::with_items(Vec::<String>::new()),
            content: TextArea::default(),
            last_char: ' ',
            mode: Mode::Normal,
            search_pattern: TextArea::default(),
            log: TextArea::default(),
        };

        let block = editor
            .log
            .block()
            .cloned()
            .unwrap_or_else(|| Block::default().borders(Borders::ALL).title(LOGFILE));
        editor.log.set_block(block);
        editor.log.set_max_histories(MAX_HISTORY_SIZE);
        editor
            .log
            .set_cursor_style(Style::default().add_modifier(Modifier::HIDDEN));

        editor
    }

    fn select_dsl_file(&mut self) {
        let dsl_file = self.files.items[self.files.state.selected().unwrap_or(0)].to_owned();
        let dsl_file_path = PathBuf::from(MODIFIEDDIR).join(&dsl_file);
        let text = fs::read_to_string(&dsl_file_path).unwrap();
        self.content = TextArea::from(text.lines());
        let block = self.content.block().cloned().unwrap_or_else(|| {
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{}", &dsl_file))
        });
        self.content.set_block(block);
        self.content.set_max_histories(MAX_HISTORY_SIZE);
        self.content
            .set_line_number_style(Style::default().bg(Color::Reset).fg(Color::White));
        self.update_log(format!("{} selected", &dsl_file).as_str());
    }

    fn next_dsl_file(&mut self) {
        self.files.next();
        self.select_dsl_file();
    }

    fn previous_dsl_file(&mut self) {
        self.files.previous();
        self.select_dsl_file();
    }

    fn update_log(&mut self, line: &str) {
        let now = Utc::now();
        let new_line = format!(
            "|{}-{:02}-{:02} {:02}:{:02}:{:02}| {}",
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
            line,
        );
        self.log.insert_newline();
        self.log.insert_str(new_line.as_str());

        // write to log file
        if let Ok(mut log_file) = OpenOptions::new()
            .read(true)
            .create(true)
            .write(true)
            .append(true)
            .open(LOGFILE)
        {
            writeln!(log_file, "{}", new_line).unwrap();
        }
    }

    fn switch_mode(&mut self, mode: Mode) {
        match mode {
            Mode::Normal => {
                self.mode = Mode::Normal;
                let style = Style::default().bg(Color::White).fg(Color::Black);
                self.content.set_cursor_style(style);
            }
            Mode::Insert => {
                self.mode = Mode::Insert;
                let style = Style::default()
                    .bg(Color::Green)
                    .fg(Color::White)
                    .add_modifier(Modifier::RAPID_BLINK)
                    .add_modifier(Modifier::BOLD);
                self.content.set_cursor_style(style);
            }
            Mode::Search => {
                self.mode = Mode::Search;
                self.search_pattern = TextArea::default();
                self.search_pattern.insert_char('/');
                self.search_pattern.set_cursor_style(
                    Style::default()
                        .bg(Color::White)
                        .fg(Color::Green)
                        .add_modifier(Modifier::RAPID_BLINK)
                        .add_modifier(Modifier::BOLD),
                );
            }
        }
    }

    fn insert_next_char(&mut self) {
        self.switch_mode(Mode::Insert);
        self.content.move_cursor(CursorMove::Forward);
    }

    fn next_line(&mut self) {
        self.content.move_cursor(CursorMove::Down);
    }

    fn previous_line(&mut self) {
        self.content.move_cursor(CursorMove::Up);
    }

    fn next_word(&mut self) {
        self.content.move_cursor(CursorMove::WordForward);
    }

    fn previous_word(&mut self) {
        self.content.move_cursor(CursorMove::WordBack);
    }

    fn next_char(&mut self) {
        self.content.move_cursor(CursorMove::Forward);
    }

    fn previous_char(&mut self) {
        self.content.move_cursor(CursorMove::Back);
    }

    fn try_to_start(&mut self) {
        if self.last_char == 'g' {
            self.content.move_cursor(CursorMove::Top);
        } else {
            self.last_char = 'g';
        }
    }

    fn to_bottom(&mut self) {
        self.content.move_cursor(CursorMove::Bottom);
        self.content.move_cursor(CursorMove::Head);
    }

    fn to_line_head(&mut self) {
        self.content.move_cursor(CursorMove::Head);
    }

    fn to_line_end(&mut self) {
        self.content.move_cursor(CursorMove::End);
    }

    fn to_next_page(&mut self) {
        self.content.scroll(Scrolling::PageDown);
    }

    fn to_previous_page(&mut self) {
        self.content.scroll(Scrolling::PageUp);
    }

    fn write(&mut self) {
        let dsl_file = self.files.items[self.files.state.selected().unwrap_or(0)].to_owned();
        let modified_dsl_file = PathBuf::from(MODIFIEDDIR).join(&dsl_file);
        let origin_dsl_file = PathBuf::from(ORIGINDIR).join(&dsl_file);
        let mut text = self.content.clone().into_lines().join("\n");
        text.push('\n');
        fs::write(&modified_dsl_file, text).expect("fail to write conent to dsl file!");

        let mut modified_dsl_file = fs::File::open(modified_dsl_file).unwrap();
        let mut origin_dsl_file = fs::File::open(origin_dsl_file).unwrap();
        let mut exist = false;

        for dslfile in self.modified.items.iter() {
            if dslfile == &dsl_file {
                exist = true;
                break;
            }
        }

        if !file_diff::diff_files(&mut modified_dsl_file, &mut origin_dsl_file) && !exist {
            self.modified.items.push(dsl_file);
            self.modified.items.sort();
        } else if file_diff::diff_files(&mut modified_dsl_file, &mut origin_dsl_file) && exist {
            match self.modified.items.binary_search(&dsl_file) {
                Ok(index) => {
                    self.modified.items.remove(index);
                }
                Err(_) => {}
            }
        }
    }

    fn insert(&mut self, key: KeyEvent) {
        if self.files.state.selected().is_none() {
            return;
        }
        self.content.input(key);
        self.write();
    }

    fn search_input(&mut self, key: KeyEvent) {
        self.search_pattern.input(key);
    }

    fn search(&mut self) {
        self.mode = Mode::Normal;
        let search_pattern = self.search_pattern.clone().into_lines().join("");
        self.content
            .set_search_pattern(search_pattern.as_str().trim_start_matches('/'))
            .unwrap();
        self.content
            .set_search_style(Style::default().bg(Color::Yellow));
    }

    fn search_forward(&mut self) {
        self.content.search_forward(false);
    }

    fn search_back(&mut self) {
        self.content.search_back(false);
    }

    fn try_delete_line(&mut self) {
        if self.last_char == 'd' {
            self.content.delete_line_by_end();
            self.content.delete_line_by_head();
            self.write();
            self.last_char = ' ';
        } else {
            self.last_char = 'd';
        }
    }

    fn try_delete_word(&mut self) {
        if self.last_char == 'd' {
            self.content.delete_next_word();
            self.write();
        } else {
            self.next_word();
        }
        self.last_char = 'w';
    }

    fn delete_char(&mut self) {
        self.content.delete_next_char();
        self.write();
    }

    fn insert_new_line_below(&mut self) {
        self.content.move_cursor(CursorMove::Head);
        self.content.move_cursor(CursorMove::Down);
        self.content.insert_newline();
        self.write();
        self.content.move_cursor(CursorMove::Up);
        self.switch_mode(Mode::Insert);
    }

    fn insert_new_line_up(&mut self) {
        self.content.move_cursor(CursorMove::Head);
        self.content.insert_newline();
        self.write();
        self.content.move_cursor(CursorMove::Up);
        self.switch_mode(Mode::Insert);
    }

    fn undo(&mut self) {
        self.content.undo();
        self.write();
    }

    fn apply(&mut self) {
        if self.modified.items.len() == 0 {
            return;
        }

        let dsl_files = self.modified.items.join(",");
        match Command::new("acpied-apply").arg(dsl_files).output() {
            Ok(o) => {
                if o.status.success() {
                    let stdout = String::from_utf8(o.stdout).unwrap();
                    for line in stdout.split("\n") {
                        if line.len() != 0 {
                            self.update_log(line);
                        }
                    }
                } else {
                    let stderr = String::from_utf8(o.stderr).unwrap();
                    for line in stderr.split("\n") {
                        self.update_log(line);
                    }
                }
            }
            Err(e) => {
                self.update_log(e.to_string().as_str());
            }
        }
    }
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    crossterm::execute!(io::stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

fn reset_terminal() -> Result<()> {
    disable_raw_mode()?;
    crossterm::execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn draw_file_list<B: Backend>(f: &mut Frame<B>, area: Rect, editor: &mut AcpiEditor) {
    let items: Vec<ListItem> = editor
        .files
        .items
        .iter()
        .map(|i| {
            let lines = vec![Spans::from(i.to_owned())];
            ListItem::new(lines).style(Style::default())
        })
        .collect();
    let items = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("ACPI TABLES")
                .title_alignment(Alignment::Center),
        )
        .highlight_style(Style::default().bg(Color::LightGreen))
        .highlight_symbol(">> ");
    f.render_stateful_widget(items, area, &mut editor.files.state);
}

fn draw_modified_list<B: Backend>(f: &mut Frame<B>, area: Rect, editor: &mut AcpiEditor) {
    let items: Vec<ListItem> = editor
        .modified
        .items
        .iter()
        .map(|i| {
            let lines = vec![Spans::from(i.to_owned())];
            ListItem::new(lines).style(Style::default())
        })
        .collect();
    let items = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("MODIFIED")
                .title_alignment(Alignment::Center),
        )
        .highlight_style(Style::default().bg(Color::LightGreen))
        .highlight_symbol(">> ");
    f.render_stateful_widget(items, area, &mut editor.files.state);
}

fn draw_search_box<B: Backend>(f: &mut Frame<B>, area: Rect, editor: &mut AcpiEditor) {
    let search_box = Paragraph::new(editor.search_pattern.clone().into_lines().join(""))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(Clear, area);
    f.render_widget(search_box, area);
}

fn draw_file_content<B: Backend>(f: &mut Frame<B>, area: Rect, editor: &mut AcpiEditor) {
    let widget = editor.content.widget();
    f.render_widget(widget, area);
}

fn draw_log<B: Backend>(f: &mut Frame<B>, area: Rect, editor: &mut AcpiEditor) {
    let widget = editor.log.widget();
    f.render_widget(widget, area);
}

fn ui<B: Backend>(f: &mut Frame<B>, editor: &mut AcpiEditor) {
    let rect = f.size();
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(1)])
        .split(rect);
    let left_side = chunks[0];
    let right_side = chunks[1];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(80), Constraint::Percentage(20)])
        .split(left_side);
    let file_list_rect = chunks[0];
    let modified_file_rect = chunks[1];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(80), Constraint::Percentage(20)])
        .split(right_side);
    let content = chunks[0];
    let log = chunks[1];

    draw_file_list(f, file_list_rect, editor);
    draw_modified_list(f, modified_file_rect, editor);
    draw_log(f, log, editor);

    match editor.mode {
        Mode::Normal | Mode::Insert => draw_file_content(f, content, editor),
        Mode::Search => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(content);
            let content = chunks[0];
            let search_box = chunks[1];
            draw_file_content(f, content, editor);
            draw_search_box(f, search_box, editor);
        }
    }
}

fn start<B: Backend>(terminal: &mut Terminal<B>, editor: &mut AcpiEditor) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, editor))?;

        match editor.mode {
            Mode::Normal => match event::read()?.into() {
                // quit the application
                Input {
                    key: Key::Char('c'),
                    ctrl: true,
                    ..
                } => return Ok(()),
                // file selection
                Input { key: Key::Up, .. } => editor.previous_dsl_file(),
                Input { key: Key::Down, .. } => editor.next_dsl_file(),
                // Cursor Movements
                Input {
                    key: Key::Char('j'),
                    ..
                } => editor.next_line(),
                Input {
                    key: Key::Char('k'),
                    ..
                } => editor.previous_line(),
                Input {
                    key: Key::Char('l'),
                    ..
                } => editor.next_char(),
                Input {
                    key: Key::Char('h'),
                    ..
                } => editor.previous_char(),
                Input {
                    key: Key::Char('b'),
                    ..
                } => editor.previous_word(),
                Input {
                    key: Key::Char('g'),
                    ..
                } => editor.try_to_start(),
                Input {
                    key: Key::Char('G'),
                    ..
                } => editor.to_bottom(),
                Input {
                    key: Key::Char('0'),
                    ..
                } => editor.to_line_head(),
                Input {
                    key: Key::Char('$'),
                    ..
                } => editor.to_line_end(),
                Input {
                    key: Key::PageUp, ..
                } => editor.to_previous_page(),
                Input {
                    key: Key::PageDown, ..
                } => editor.to_next_page(),
                // search
                Input {
                    key: Key::Char('n'),
                    ..
                } => editor.search_forward(),
                Input {
                    key: Key::Char('N'),
                    ..
                } => editor.search_back(),
                // switch mode
                Input {
                    key: Key::Char('i'),
                    ..
                } => editor.switch_mode(Mode::Insert),
                Input {
                    key: Key::Char('a'),
                    ctrl: false,
                    ..
                } => editor.insert_next_char(),
                Input {
                    key: Key::Char('o'),
                    ..
                } => editor.insert_new_line_below(),
                Input {
                    key: Key::Char('O'),
                    ..
                } => editor.insert_new_line_up(),
                // edit
                Input {
                    key: Key::Char('d'),
                    ..
                } => editor.try_delete_line(),
                Input {
                    key: Key::Char('w'),
                    ..
                } => editor.try_delete_word(),
                Input {
                    key: Key::Char('x'),
                    ..
                } => editor.delete_char(),
                Input {
                    key: Key::Char('u'),
                    ..
                } => editor.undo(),
                // into search mode
                Input {
                    key: Key::Char('/'),
                    ..
                } => editor.switch_mode(Mode::Search),
                // apply
                Input {
                    key: Key::Char('a'),
                    ctrl: true,
                    ..
                } => editor.apply(),
                _ => {}
            },
            Mode::Insert => {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Esc => editor.switch_mode(Mode::Normal),
                        _ => {}
                    }
                    editor.insert(key);
                }
            }
            Mode::Search => {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Esc => editor.switch_mode(Mode::Normal),
                        KeyCode::Enter => editor.search(),
                        _ => {}
                    }
                    editor.search_input(key);
                }
            }
        }
    }
}

fn check_executable(executable: &str) {
    let output = Command::new("which")
        .arg(executable)
        .output()
        .expect(format!("fail to check {}!", executable).as_str());
    if !output.status.success() {
        eprintln!("{}", format!("{} not found!", executable));
        exit(1);
    }
}

fn check_user() {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .expect("fail to check user!");
    if !output.status.success() {
        eprintln!("check user failed");
    } else {
        if String::from_utf8(output.stdout.to_owned())
            .unwrap()
            .trim_end()
            != String::from("0")
        {
            eprintln!("acpied must be run as root!");
            exit(1);
        }
    }
}

fn check_prerequisites() {
    check_user();
    check_executable("grubby");
    check_executable("acpidump");
    check_executable("acpixtract");
    check_executable("iasl");
}

pub fn run() -> Result<()> {
    check_prerequisites();
    let mut editor = AcpiEditor::new();
    let mut terminal = init_terminal()?;
    let result = start(&mut terminal, &mut editor);
    reset_terminal()?;
    if let Err(err) = result {
        println!("{:?}", err);
    }
    Ok(())
}

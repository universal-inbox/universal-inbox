//! Terminal views of the data encryption commands: a live progress view of
//! `encrypt-plaintext` / `reencrypt` and the status table. Drawn in an inline viewport (no raw
//! mode, no alternate screen) so the output stays in the scrollback and Ctrl-C works as usual.

use std::{
    collections::BTreeSet,
    io::{self, Stdout},
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use ratatui::{
    Frame, Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Cell, Gauge, Row, Table},
};
use tracing::warn;

use crate::{
    universal_inbox::{
        UniversalInboxError,
        data_encryption::{DataEncryptionProgress, DataEncryptionStatus, LEGACY_TOKEN_KEY_LABEL},
    },
    utils::crypto::{DataKeyring, KeyId, LEGACY_KEY_ID},
};

fn terminal_error(err: io::Error) -> UniversalInboxError {
    UniversalInboxError::Unexpected(anyhow!("Failed to draw in the terminal: {err}"))
}

/// Inline terminal leaving the cursor visible below its viewport when dropped, including on
/// errors and panics
pub struct InlineTerminal(Terminal<CrosstermBackend<Stdout>>);

impl InlineTerminal {
    fn new(height: u16) -> Result<Self, UniversalInboxError> {
        let mut terminal = Terminal::with_options(
            CrosstermBackend::new(io::stdout()),
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        )
        .map_err(terminal_error)?;
        // Blank cells are not drawn: clear what the viewport covers
        terminal.clear().map_err(terminal_error)?;
        Ok(Self(terminal))
    }

    fn draw(&mut self, render: impl FnOnce(&mut Frame)) -> Result<(), UniversalInboxError> {
        self.0.draw(render).map(|_| ()).map_err(terminal_error)
    }
}

impl Drop for InlineTerminal {
    fn drop(&mut self) {
        let last_row = self.0.get_frame().area().bottom().saturating_sub(1);
        self.0.set_cursor_position((0, last_row)).ok();
        self.0.show_cursor().ok();
        println!();
    }
}

#[derive(Debug)]
struct ColumnProgress {
    column: String,
    total: u64,
    done: u64,
    started: Option<Instant>,
    finished: Option<Instant>,
}

#[derive(Debug)]
struct ProgressState {
    columns: Vec<ColumnProgress>,
    started: Instant,
    /// When the last column finished (or the run started): the next column starts from there
    last_mark: Instant,
}

impl ProgressState {
    fn column(&mut self, column: &str, now: Instant) -> &mut ColumnProgress {
        let index = match self.columns.iter().position(|c| c.column == column) {
            Some(index) => index,
            None => {
                self.columns.push(ColumnProgress {
                    column: column.to_string(),
                    total: 0,
                    done: 0,
                    started: None,
                    finished: None,
                });
                self.columns.len() - 1
            }
        };
        let last_mark = self.last_mark;
        let progress = &mut self.columns[index];
        progress.started.get_or_insert(last_mark.min(now));
        progress
    }

    fn batch_done(&mut self, column: &str, processed: u64, now: Instant) {
        self.column(column, now).done += processed;
    }

    fn column_done(&mut self, column: &str, now: Instant) {
        self.column(column, now).finished = Some(now);
        self.last_mark = now;
    }
}

/// Values per second processed between `from` and `to`
fn rate(done: u64, from: Instant, to: Instant) -> Option<f64> {
    let elapsed = to.duration_since(from).as_secs_f64();
    (done > 0 && elapsed > 0.0).then(|| done as f64 / elapsed)
}

fn eta(remaining: u64, rate: Option<f64>) -> String {
    match rate {
        _ if remaining == 0 => "-".to_string(),
        Some(rate) => format_duration(Duration::from_secs_f64(remaining as f64 / rate)),
        None => "?".to_string(),
    }
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m{:02}s", secs / 60, secs % 60),
        _ => format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60),
    }
}

fn format_rate(rate: Option<f64>) -> String {
    rate.map(|rate| format!("{rate:.0}/s"))
        .unwrap_or_else(|| "-".to_string())
}

/// Live progress of `encrypt-plaintext` / `reencrypt`, fed by the service through
/// [`DataEncryptionProgress`]
pub struct ProgressView {
    title: String,
    state: Mutex<ProgressState>,
}

impl ProgressView {
    /// `totals`: number of values to process per column, in processing order
    pub fn new(title: &str, totals: Vec<(String, i64)>) -> Self {
        Self::new_at(title, totals, Instant::now())
    }

    fn new_at(title: &str, totals: Vec<(String, i64)>, now: Instant) -> Self {
        Self {
            title: title.to_string(),
            state: Mutex::new(ProgressState {
                columns: totals
                    .into_iter()
                    .map(|(column, total)| ColumnProgress {
                        column,
                        total: total.max(0) as u64,
                        done: 0,
                        started: None,
                        finished: None,
                    })
                    .collect(),
                started: now,
                last_mark: now,
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ProgressState> {
        self.state.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn terminal(&self) -> Result<InlineTerminal, UniversalInboxError> {
        // Borders, table header and gauge
        InlineTerminal::new(self.state().columns.len() as u16 + 4)
    }

    pub fn draw(&self, terminal: &mut InlineTerminal) -> Result<(), UniversalInboxError> {
        terminal.draw(|frame| self.render(frame, Instant::now()))
    }

    fn render(&self, frame: &mut Frame, now: Instant) {
        let state = self.state();
        let rows = state.columns.iter().map(|column| {
            let to = column.finished.unwrap_or(now);
            let column_rate = column.started.and_then(|from| rate(column.done, from, to));
            let (status, style) = match (column.started, column.finished) {
                (_, Some(_)) => ("done", Style::new().green()),
                (Some(_), None) => ("running", Style::new().yellow()),
                (None, None) => ("pending", Style::new().dim()),
            };
            Row::new([
                Cell::from(column.column.clone()),
                Cell::from(Line::from(format!("{}/{}", column.done, column.total)).right_aligned()),
                Cell::from(Line::from(format_rate(column_rate)).right_aligned()),
                Cell::from(
                    Line::from(if column.finished.is_some() {
                        "-".to_string()
                    } else {
                        eta(column.total.saturating_sub(column.done), column_rate)
                    })
                    .right_aligned(),
                ),
                Cell::from(status),
            ])
            .style(style)
        });
        let name_width = state
            .columns
            .iter()
            .map(|column| column.column.len())
            .max()
            .unwrap_or(0) as u16;
        let table = Table::new(
            rows,
            [
                Constraint::Length(name_width.max(6)),
                Constraint::Length(15),
                Constraint::Length(9),
                Constraint::Length(7),
                Constraint::Length(7),
            ],
        )
        .header(Row::new(["Column", "Done/Total", "Rate", "ETA", "Status"]).bold())
        .block(Block::bordered().title(self.title.as_str()));

        let total: u64 = state.columns.iter().map(|column| column.total).sum();
        let done: u64 = state.columns.iter().map(|column| column.done).sum();
        let all_finished = state.columns.iter().all(|column| column.finished.is_some());
        let ratio = if total == 0 {
            if all_finished { 1.0 } else { 0.0 }
        } else {
            (done as f64 / total as f64).min(1.0)
        };
        let overall_rate = rate(done, state.started, now);
        let gauge = Gauge::default()
            .gauge_style(Style::new().cyan())
            .ratio(ratio)
            .label(format!(
                "{done}/{total} · {} · ETA {}",
                format_rate(overall_rate),
                if all_finished {
                    "-".to_string()
                } else {
                    eta(total.saturating_sub(done), overall_rate)
                }
            ));

        let [table_area, gauge_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        frame.render_widget(table, table_area);
        frame.render_widget(gauge, gauge_area);
    }
}

impl DataEncryptionProgress for ProgressView {
    fn batch_done(&self, column: &str, processed: u64) {
        self.state().batch_done(column, processed, Instant::now());
    }

    fn column_done(&self, column: &str) {
        self.state().column_done(column, Instant::now());
    }
}

/// Which key labels of [`ColumnStatus::values_per_key`] are active or unreadable
///
/// [`ColumnStatus::values_per_key`]: crate::universal_inbox::data_encryption::ColumnStatus::values_per_key
struct KeyStatus {
    active: String,
    unreadable: BTreeSet<String>,
}

impl KeyStatus {
    fn new(keyring: &DataKeyring, labels: &[String]) -> Self {
        Self {
            active: keyring.active_key_id().to_string(),
            unreadable: labels
                .iter()
                .filter(|label| !keyring.has_key(label.parse::<KeyId>().unwrap_or(LEGACY_KEY_ID)))
                .cloned()
                .collect(),
        }
    }
}

/// Key labels used by `status`, key ids in numeric order then legacy
fn key_labels(status: &DataEncryptionStatus) -> Vec<String> {
    let mut labels = status.values_per_key().into_keys().collect::<Vec<_>>();
    labels.sort_by_key(|label| {
        (
            label == LEGACY_TOKEN_KEY_LABEL,
            label.parse::<KeyId>().ok(),
            label.clone(),
        )
    });
    labels
}

fn count_cell(count: i64, style: Style) -> Cell<'static> {
    if count == 0 {
        Cell::from(Line::from("-").right_aligned()).dim()
    } else {
        Cell::from(Line::from(count.to_string()).right_aligned()).style(style)
    }
}

fn key_title(label: &str, keys: &KeyStatus) -> String {
    match label {
        LEGACY_TOKEN_KEY_LABEL => label.to_string(),
        _ if label == keys.active => format!("key {label} (active)"),
        _ => format!("key {label}"),
    }
}

fn status_table_height(status: &DataEncryptionStatus) -> u16 {
    // Borders, header and totals
    status.columns.len() as u16 + 4
}

fn render_status(frame: &mut Frame, status: &DataEncryptionStatus, keys: &KeyStatus) {
    let labels = key_labels(status);
    let key_style = |label: &str| {
        if keys.unreadable.contains(label) {
            Style::new().red().bold()
        } else if label != keys.active {
            Style::new().yellow()
        } else {
            Style::new()
        }
    };
    let plaintext_style = Style::new().yellow();
    let undecryptable_style = Style::new().red().bold();

    let mut header = vec![
        Cell::from("Column"),
        Cell::from(Line::from("Plaintext").right_aligned()),
    ];
    header.extend(labels.iter().map(|label| {
        let cell = Cell::from(Line::from(key_title(label, keys)).right_aligned());
        if *label == keys.active {
            cell.green()
        } else {
            cell
        }
    }));
    header.push(Cell::from(Line::from("Undecryptable").right_aligned()));

    let mut rows = status
        .columns
        .iter()
        .map(|column| {
            let mut cells = vec![
                Cell::from(column.column.clone()),
                count_cell(column.plaintext_values, plaintext_style),
            ];
            cells.extend(labels.iter().map(|label| {
                count_cell(
                    column.values_per_key.get(label).copied().unwrap_or(0),
                    key_style(label),
                )
            }));
            cells.push(count_cell(column.undecryptable_values, undecryptable_style));
            Row::new(cells)
        })
        .collect::<Vec<_>>();

    let per_key = status.values_per_key();
    let mut totals = vec![
        Cell::from("Total"),
        count_cell(status.plaintext_values(), plaintext_style),
    ];
    totals.extend(
        labels
            .iter()
            .map(|label| count_cell(per_key.get(label).copied().unwrap_or(0), key_style(label))),
    );
    totals.push(count_cell(
        status
            .columns
            .iter()
            .map(|column| column.undecryptable_values)
            .sum(),
        undecryptable_style,
    ));
    rows.push(Row::new(totals).bold());

    let name_width = status
        .columns
        .iter()
        .map(|column| column.column.len())
        .max()
        .unwrap_or(0) as u16;
    let mut widths = vec![
        Constraint::Length(name_width.max(6)),
        Constraint::Length(10),
    ];
    widths.extend(
        labels
            .iter()
            .map(|label| Constraint::Length((key_title(label, keys).len() as u16).max(6))),
    );
    widths.push(Constraint::Length(13));

    let table = Table::new(rows, widths)
        .header(Row::new(header).bold())
        .block(Block::bordered().title(format!(
            "Data encryption status · active key {}",
            keys.active
        )));
    frame.render_widget(table, frame.area());
}

/// Print `status` as a table, then the values the configured keys cannot read
pub fn print_status(
    status: &DataEncryptionStatus,
    keyring: &DataKeyring,
) -> Result<(), UniversalInboxError> {
    let keys = KeyStatus::new(keyring, &key_labels(status));
    let drawn = InlineTerminal::new(status_table_height(status))
        .and_then(|mut terminal| terminal.draw(|frame| render_status(frame, status, &keys)));
    if let Err(err) = drawn {
        warn!("Cannot draw the status table, printing it plain: {err}");
        println!("{status}");
        return Ok(());
    }
    for problem in status.unreadable_values(keyring) {
        println!("{problem}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;
    use ratatui::{
        backend::TestBackend,
        buffer::Buffer,
        style::{Color, Modifier},
    };

    use crate::universal_inbox::data_encryption::ColumnStatus;

    use super::*;

    fn buffer_lines(buffer: &Buffer) -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// Style of the first cell of `text` in the rendered `buffer`
    fn style_of(buffer: &Buffer, line: &str, text: &str) -> (Color, Modifier) {
        let lines = buffer_lines(buffer);
        let y = lines
            .iter()
            .position(|l| l.contains(line))
            .unwrap_or_else(|| panic!("no line containing {line:?} in {lines:#?}"));
        let x = lines[y].find(text).unwrap();
        // Box-drawing characters are multi-byte: count characters, not bytes
        let x = lines[y][..x].chars().count();
        let cell = &buffer[(x as u16, y as u16)];
        (cell.fg, cell.modifier)
    }

    fn status() -> DataEncryptionStatus {
        DataEncryptionStatus {
            columns: vec![
                ColumnStatus {
                    column: "task.body_enc".to_string(),
                    plaintext_values: 0,
                    values_per_key: BTreeMap::from([("2".to_string(), 7), ("3".to_string(), 11)]),
                    undecryptable_values: 0,
                },
                ColumnStatus {
                    column: "integration_connection.access_token".to_string(),
                    plaintext_values: 5,
                    values_per_key: BTreeMap::from([
                        ("3".to_string(), 13),
                        (LEGACY_TOKEN_KEY_LABEL.to_string(), 17),
                    ]),
                    undecryptable_values: 19,
                },
            ],
        }
    }

    fn render_status_buffer(keys: &KeyStatus) -> Buffer {
        let status = status();
        let mut terminal =
            Terminal::new(TestBackend::new(120, status_table_height(&status))).unwrap();
        terminal
            .draw(|frame| render_status(frame, &status, keys))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn test_render_status_table() {
        let buffer = render_status_buffer(&KeyStatus {
            active: "3".to_string(),
            unreadable: BTreeSet::new(),
        });
        let lines = buffer_lines(&buffer);

        assert!(lines[0].contains("active key 3"), "{lines:#?}");
        let header = &lines[1];
        let key_2 = header.find("key 2").unwrap();
        let key_3 = header.find("key 3 (active)").unwrap();
        let legacy = header.find("legacy").unwrap();
        assert!(key_2 < key_3 && key_3 < legacy, "{header}");
        let totals = lines.iter().find(|line| line.contains("Total")).unwrap();
        for total in ["5", "7", "24", "17", "19"] {
            assert!(
                totals.split_whitespace().any(|word| word == total),
                "{total} not in {totals}"
            );
        }

        // Values to re-encrypt in yellow, the active key ones unstyled
        assert_eq!(style_of(&buffer, "task.body_enc", "7").0, Color::Yellow);
        assert_eq!(style_of(&buffer, "task.body_enc", "11").0, Color::Reset);
        assert_eq!(
            style_of(&buffer, "integration_connection.access_token", "17").0,
            Color::Yellow
        );
        assert_eq!(
            style_of(&buffer, "integration_connection.access_token", "19"),
            (Color::Red, Modifier::BOLD)
        );
        assert_eq!(
            style_of(&buffer, "integration_connection.access_token", "5").0,
            Color::Yellow
        );
    }

    #[test]
    fn test_render_status_table_unreadable_key() {
        let buffer = render_status_buffer(&KeyStatus {
            active: "3".to_string(),
            unreadable: BTreeSet::from(["2".to_string()]),
        });

        assert_eq!(
            style_of(&buffer, "task.body_enc", "7"),
            (Color::Red, Modifier::BOLD)
        );
    }

    #[test]
    fn test_render_progress() {
        let start = Instant::now();
        let view = ProgressView::new_at(
            "Re-encrypting with key 3",
            vec![
                ("task.body_enc".to_string(), 100),
                ("user.email_enc".to_string(), 1000),
                ("third_party_item.data_enc".to_string(), 50),
            ],
            start,
        );
        {
            let mut state = view.state();
            state.batch_done("task.body_enc", 100, start + Duration::from_secs(5));
            state.column_done("task.body_enc", start + Duration::from_secs(10));
            state.batch_done("user.email_enc", 200, start + Duration::from_secs(20));
        }

        let mut terminal = Terminal::new(TestBackend::new(80, 7)).unwrap();
        terminal
            .draw(|frame| view.render(frame, start + Duration::from_secs(20)))
            .unwrap();
        let lines = buffer_lines(terminal.backend().buffer());

        let row = |column: &str| {
            lines
                .iter()
                .find(|line| line.contains(column))
                .unwrap()
                .split_whitespace()
                .skip(1)
                .take(4)
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(row("task.body_enc"), "100/100 10/s - done");
        // 200 values in 10s since the previous column finished: 800 left at 20/s
        assert_eq!(row("user.email_enc"), "200/1000 20/s 40s running");
        assert_eq!(row("third_party_item.data_enc"), "0/50 - ? pending");
        // 300 values in 20s overall: 850 left at 15/s
        assert!(
            lines
                .iter()
                .any(|line| line.contains("300/1150 · 15/s · ETA 56s")),
            "{lines:#?}"
        );
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(42)), "42s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1m05s");
        assert_eq!(format_duration(Duration::from_secs(7322)), "2h02m");
    }
}

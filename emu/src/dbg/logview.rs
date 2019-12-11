use super::uisupport::{ctext, ImGuiListClipper};
use super::UiCtx;
use crate::log::{
    LogDrain, LogPrinter, LogRecordPrinter, ThreadSafeTimestampFn, KEY_FRAME, KEY_PC, KEY_SUBSYSTEM,
};
use sdl2::keyboard::Scancode;

use imgui::*;

use rusqlite::{params, Connection, Statement, NO_PARAMS};
use slog;
use slog::*;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::io;
use std::io::Write;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const LOG_LEVEL_COLOR: [[f32; 4]; 7] = [
    [255.0 / 255.0, 255.0 / 255.0, 255.0 / 255.0, 255.0], // none
    [231.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0, 255.0],   // critical
    [231.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0, 255.0],   // error
    [230.0 / 255.0, 219.0 / 255.0, 116.0 / 255.0, 255.0], // warning
    [165.0 / 255.0, 224.0 / 255.0, 46.0 / 255.0, 255.0],  // info
    [255.0 / 255.0, 255.0 / 255.0, 255.0 / 255.0, 255.0], // debug
    [102.0 / 255.0, 99.0 / 255.0, 83.0 / 255.0, 255.0],   // trace
];

const COLOR_MODULE: [f32; 4] = [174.0 / 129.0, 129.0 / 255.0, 255.0 / 255.0, 255.0];
const COLOR_FRAME: [f32; 4] = [95.0 / 129.0, 158.0 / 255.0, 160.0 / 255.0, 255.0];

#[derive(Default)]
pub(crate) struct LogLine {
    level: u8,
    frame: u32,
    module: u8,
    location: Option<(String, u64)>,
    msg: String,
    kv: String,
}

impl LogLine {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<LogLine> {
        Ok(LogLine {
            level: row.get(1)?,
            frame: row.get(2)?,
            module: row.get(3)?,
            msg: row.get(4)?,
            kv: row.get(5)?,
            location: None,
        })
    }
}

// LogFilter is a filter in the logpool that allwos to extract a subset of logs
#[derive(Default)]
struct LogFilter {
    min_level: Option<u8>,    // Minimum logging level to be displayed
    max_level: Option<u8>,    // Maximum logging level to be displayed
    min_frame: Option<u32>,   // Minimum frame to be displayed
    max_frame: Option<u32>,   // Maximum frame to be displayed
    modules: Option<Vec<u8>>, // List of modules that must be displayed
    text: Option<String>,     // Full text search in msg+kv
}

// LogPool is a in-memory log buffer that collects logs from slog and stores them
// for efficient filtering/searching and displaying. It's used in the debugger to
// collect all the logs and display them in the logview.
//
// Use new_debugger_logger() to create a LogPool and a slog::Logger connected to it.
pub struct LogPool {
    modules: Vec<ImString>,
    modidx: HashMap<String, u8>,
    conn: Connection,
    num_lines: usize,
    filter: LogFilter,
    has_filter: bool,
    last_count: usize,
    last_count_rowid: i64,
    send_analyze: Sender<bool>,
}

pub type LogPoolPtr = Arc<Mutex<Box<LogPool>>>;

impl LogPool {
    fn new() -> LogPoolPtr {
        let conn = Connection::open("file:logpool1?mode=memory&cache=shared").unwrap();
        conn.execute(
            "CREATE TABLE log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            level INTEGER,
            frame INTEGER,
            module INTEGER,
            msg TEXT NOT NULL,
            kv TEXT NOT NULL
        )",
            NO_PARAMS,
        )
        .unwrap();
        conn.execute("CREATE INDEX multi ON log (module,frame,level)", NO_PARAMS)
            .unwrap();

        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let conn = Connection::open("file:logpool1?mode=memory&cache=shared").unwrap();
            let mut last = Instant::now();
            loop {
                match receiver.recv() {
                    Ok(_) => {
                        // Debounce to avoid wasting too much CPU
                        if last.elapsed().as_secs() > 5 {
                            conn.execute("ANALYZE", NO_PARAMS).unwrap();
                            last = Instant::now();
                        }
                    }
                    Err(_) => return,
                }
            }
        });

        let mut pool = LogPool {
            modidx: HashMap::new(),
            modules: Vec::new(),
            conn: conn,
            has_filter: false,
            num_lines: 0,
            last_count: 0,
            last_count_rowid: 0,
            filter: Default::default(),
            send_analyze: sender,
        };
        pool.update_filter();
        Arc::new(Mutex::new(Box::new(pool)))
    }

    fn update_filter(&mut self) {
        self.conn
            .execute("DROP VIEW IF EXISTS log_filter", NO_PARAMS)
            .unwrap();
        let mut view = "CREATE VIEW log_filter AS SELECT
            id, level, frame, module, msg, kv FROM log "
            .to_string();
        self.has_filter = false;

        // Now build all the filters in the WHERE clause.
        // Unfortunately, SQLite does not support params in view
        // statements, so we must manually interpolate the arguments.
        let has_level = self.filter.min_level.is_some() || self.filter.max_level.is_some();
        let has_module = self.filter.modules.is_some();
        let has_frame = self.filter.min_frame.is_some() || self.filter.max_frame.is_some();

        let mut prefix = "WHERE ";

        if let Some(ref modules) = self.filter.modules {
            self.has_filter = true;
            view = view + prefix + "module IN (";
            for (n, m) in modules.iter().enumerate() {
                if n != 0 {
                    view += ",";
                }
                view += &format!("{}", (*m));
            }
            view += ") ";
            prefix = "AND ";
        }

        if let Some(min_frame) = self.filter.min_frame {
            self.has_filter = true;
            view = view + prefix + &format!("frame >= {} ", min_frame);
            prefix = "AND ";
        }
        if let Some(max_frame) = self.filter.max_frame {
            self.has_filter = true;
            view = view + prefix + &format!("frame <= {} ", max_frame);
            prefix = "AND ";
        }

        if let Some(min_level) = self.filter.min_level {
            self.has_filter = true;
            view = view + prefix + &format!("level >= {} ", min_level);
            prefix = "AND ";
        }
        if let Some(max_level) = self.filter.max_level {
            self.has_filter = true;
            view = view + prefix + &format!("level <= {} ", max_level);
            prefix = "AND ";
        }

        if let Some(ref text) = self.filter.text {
            self.has_filter = true;
            let text = text.replace("'", "''");
            view = view + prefix + &format!("(msg LIKE '%{}%' OR kv LIKE '%{}%')", text, text);
        }
        self.conn.execute(&view, NO_PARAMS).unwrap();
        self.last_count_rowid = 0;
        self.last_count = 0;
    }

    fn log(&mut self, line: LogLine) {
        let mut stmt = self
            .conn
            .prepare_cached("INSERT INTO log (level, frame, module, msg, kv) VALUES (?,?,?,?,?)")
            .unwrap();
        stmt.execute(params![
            line.level,
            line.frame,
            line.module,
            line.msg,
            line.kv
        ])
        .unwrap();

        // Run a background optimization every 64K lines.
        self.num_lines += 1;
        if self.num_lines % 64 * 1204 == 0 {
            self.send_analyze.send(true).unwrap();
        }
    }

    // Returns loglines from the current filter; [first, last] is the inclusive range of
    // loglines indices that specify which portion of the filtered lines will be extracted.
    fn filter_get(&mut self, first: u32, last: u32) -> Vec<LogLine> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT * FROM log_filter ORDER BY id LIMIT ? OFFSET ?")
            .unwrap();
        let mut lines = Vec::new();
        for row in stmt
            .query_map(params![last - first + 1, first], LogLine::from_row)
            .unwrap()
        {
            lines.push(row.unwrap());
        }
        lines
    }

    // Returns the last n lines of the filter, useful for "following" the loglines.
    // This is semantically equivalent to `fiter_get(filter_count()-n, filter_count())`
    // but it's much faster.
    fn filter_last(&mut self, n: usize) -> Vec<LogLine> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT * FROM log_filter ORDER by id DESC LIMIT ?")
            .unwrap();
        let mut lines = Vec::new();
        for row in stmt
            .query_map(params![n as u32], LogLine::from_row)
            .unwrap()
        {
            lines.push(row.unwrap());
        }
        lines.reverse();
        lines
    }

    // Returns the number of loglines in the active filter.
    // Since calculating the total count can be slow when the filter contains,
    // many elements, this function works incrementally: it caches the last
    // computed count, and updates it counting only the new loglines
    // arrived since the previous call.
    fn filter_count(&mut self) -> usize {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT COUNT(*) FROM log_filter WHERE id > ?")
            .unwrap();
        let mut rows = stmt.query(params![self.last_count_rowid]).unwrap();
        let row = rows.next().unwrap().unwrap();
        self.last_count += row.get_unwrap::<usize, u32>(0) as usize;

        let mut stmt = self
            .conn
            .prepare_cached("SELECT last_insert_rowid()")
            .unwrap();
        let mut rows = stmt.query(NO_PARAMS).unwrap();
        let row = rows.next().unwrap().unwrap();
        self.last_count_rowid = row.get_unwrap::<usize, i64>(0);

        self.last_count
    }

    // Returns true if filter_count() is very fast
    fn fast_filter_count(&mut self) -> bool {
        self.filter.text.is_none() && false
    }

    fn last(&mut self) -> LogLine {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT MAX(id), level, frame, module, msg, kv FROM log")
            .unwrap();
        let mut rows = stmt.query(NO_PARAMS).unwrap();
        let row = rows.next().unwrap().unwrap();

        LogLine {
            level: row.get_unwrap(1),
            frame: row.get_unwrap(2),
            module: row.get_unwrap(3),
            msg: row.get_unwrap(4),
            kv: row.get_unwrap(5),
            location: None,
        }
    }
}

struct DebugRecordPrinter {
    pool: LogPoolPtr,

    line: LogLine,
    pc: Option<u64>,
    sub: Option<String>,
    kv: Vec<u8>,
}

impl LogRecordPrinter for DebugRecordPrinter {
    fn print_header(
        &mut self,
        record: &Record,
        fn_timestamp: &ThreadSafeTimestampFn<Output = io::Result<()>>,
    ) -> io::Result<()> {
        let mut pool = self.pool.lock().unwrap();
        self.line.level = record.level().as_usize().try_into().unwrap();

        let tag = if !record.tag().is_empty() {
            record.tag()
        } else {
            record.module()
        };

        let num_modules = pool.modules.len();
        let mut insert = false;
        self.line.module = match pool.modidx.entry(tag.to_string()) {
            Entry::Occupied(e) => *e.get(),
            Entry::Vacant(e) => {
                insert = true;
                *e.insert(num_modules.try_into().unwrap())
            }
        };
        if insert {
            pool.modules
                .push(im_str!("[{}]", tag.to_string()).to_owned());
        }
        self.line.frame = 0; // FIXME
        self.line.msg = record.msg().to_string();
        Ok(())
    }

    fn print_kv<K: fmt::Display, V: fmt::Display>(&mut self, k: K, v: V) -> io::Result<()> {
        let k = format!("{}", k);
        let v = format!("{}", v);
        match k.as_ref() {
            KEY_FRAME => self.line.frame = u32::from_str_radix(&v, 10).unwrap_or(0),
            KEY_PC => self.pc = u64::from_str_radix(&v, 16).ok(),
            KEY_SUBSYSTEM => self.sub = Some(v),
            _ => {
                if !self.kv.is_empty() {
                    write!(&mut self.kv, "\t")?;
                }
                write!(&mut self.kv, "{}={}", k, v)?;
            }
        };
        Ok(())
    }

    fn finish(self) -> io::Result<()> {
        use std::str;
        let mut line = self.line;
        line.kv = str::from_utf8(&self.kv).unwrap().to_owned();
        line.location = match (self.sub, self.pc) {
            (Some(sub), Some(pc)) => Some((sub, pc)),
            _ => None,
        };

        self.pool.lock().unwrap().log(line);
        Ok(())
    }
}

struct DebugPrinter {
    pool: LogPoolPtr,
}

impl DebugPrinter {
    fn new(pool: LogPoolPtr) -> DebugPrinter {
        DebugPrinter { pool }
    }
}

impl LogPrinter for DebugPrinter {
    type RecordPrinter = DebugRecordPrinter;

    fn with_record<F>(&self, _record: &Record, f: F) -> io::Result<()>
    where
        F: FnOnce(Self::RecordPrinter) -> io::Result<()>,
    {
        f(DebugRecordPrinter {
            pool: self.pool.clone(),
            line: LogLine::default(),
            kv: Vec::new(),
            pc: None,
            sub: None,
        })
    }
}

/// Create a logger whose output is piped into an in-memory buffer (LogPool).
/// This is useful to display the logging within the debugger, by passing
/// the logging pool to the debugger.
pub fn new_debugger_logger() -> (slog::Logger, LogPoolPtr) {
    let pool = LogPool::new();
    let printer = DebugPrinter::new(pool.clone());
    let drain = LogDrain::new(printer).build().fuse();
    let logger = slog::Logger::root(drain, o!());
    (logger, pool)
}

pub(crate) fn render_logview<'a, 'ui>(ui: &'a Ui<'ui>, ctx: &mut UiCtx, pool: LogPoolPtr) {
    Window::new(&im_str!("Logs"))
        .size([500.0, 300.0], Condition::FirstUseEver)
        .build(ui, || {
            let mut pool = pool.lock().unwrap();
            let mut update_filter = false;

            // Activate / deactivate follow mode
            let mut following_changed = ui.checkbox(im_str!("Follow"), &mut ctx.logview.following);
            if ui.is_item_hovered() {
                ui.tooltip_text(im_str!("Display new loglines as they arrive"));
            }
            if !ui.io().want_text_input && ui.is_key_pressed(Scancode::F as _) {
                ctx.logview.following = !ctx.logview.following;
                following_changed = true;
            }
            ui.same_line(0.0);

            // Select minimum visible logging level
            let levels: [&ImStr; 7] = [
                im_str!("Off (ALL)"),
                im_str!("Critical (CRIT)"),
                im_str!("Error (ERRO)"),
                im_str!("Warning (WARN)"),
                im_str!("Info (INFO)"),
                im_str!("Debug (DEBG)"),
                im_str!("Trace (TRCE)"),
            ];
            let mut max_level = match pool.filter.max_level {
                None => 0,
                Some(ml) => ml as usize,
            };
            ui.set_next_item_width(130.0);
            if ComboBox::new(im_str!("##level")).build_simple_string(&ui, &mut max_level, &levels) {
                if max_level == 0 {
                    pool.filter.max_level = None;
                } else {
                    pool.filter.max_level = Some(max_level as u8);
                }
                update_filter = true;
            }
            if ui.is_item_hovered() {
                ui.tooltip_text(im_str!("Minimum displayed level"));
            }
            ui.same_line(0.0);

            // Filter by module
            if ui.button(im_str!("Modules.."), [0.0, 0.0]) {
                ui.open_popup(im_str!("##modules"));
            }
            if ui.is_item_hovered() {
                ui.tooltip_text(im_str!("Show/hide specific modules"));
            }
            ui.popup(im_str!("##modules"), || {
                if ui.button(im_str!("Show all"), [ui.window_content_region_width(), 0.0]) {
                    pool.filter.modules = None;
                    update_filter = true;
                    ui.close_current_popup();
                }
                ui.separator();

                let mut selected = Vec::with_capacity(pool.modules.len());
                if let Some(ref modules) = pool.filter.modules {
                    selected.resize(pool.modules.len(), false);
                    for m in modules.iter() {
                        selected[*m as usize] = true;
                    }
                } else {
                    selected.resize(pool.modules.len(), true);
                }

                let mut changed = false;
                for (n, m) in pool.modules.iter().enumerate() {
                    if ui.checkbox(&im_str!("{}", m), &mut selected[n]) {
                        changed = true;
                    }
                }
                if changed {
                    let numsel = selected.iter().filter(|v| **v).count();
                    if numsel == selected.len() {
                        pool.filter.modules = None;
                    } else {
                        pool.filter.modules = Some(
                            selected
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, v)| Some(idx as u8).filter(|_| *v))
                                .collect(),
                        );
                    }
                    update_filter = true;
                }
            });
            ui.same_line(0.0);

            let right_section = ui.window_size()[0] - 80.0;

            // Full-text search in logs
            ui.set_next_item_width(200.0_f32.min(right_section - ui.cursor_pos()[0] - 20.0));
            let mut text_filter = ImString::with_capacity(64);
            if let Some(ref text) = pool.filter.text {
                text_filter.push_str(&text);
            }
            if InputText::new(ui, im_str!("##textfilter"), &mut text_filter)
                .auto_select_all(true)
                //.hint(im_str!("search..."))   // FIXME: not implemented yet
                .build()
            {
                if text_filter.to_str().len() == 0 {
                    pool.filter.text = None;
                } else {
                    pool.filter.text = Some(text_filter.to_str().to_string());
                }
                update_filter = true;
            }
            if ui.is_item_hovered() {
                ui.tooltip_text(im_str!("Search within logs"));
            }
            ui.same_line(right_section);

            // Simple count of the displayed log-lines
            let numlines = ctx.logview.filter_count.unwrap_or(0);
            if numlines >= 1000000 {
                ui.text(im_str!("Logs: {:.1}M", numlines as f32 / 1000000.0));
            } else if numlines >= 10000 {
                ui.text(im_str!("Logs: {}K", numlines / 1000));
            } else if numlines >= 1000 {
                ui.text(im_str!("Logs: {:.1}K", numlines as f32 / 1000.0));
            } else {
                ui.text(im_str!("Logs: {}", numlines));
            }

            ui.separator();

            ChildWindow::new(&im_str!("##scrolling"))
                .size([0.0, 0.0])
                .always_horizontal_scrollbar(true)
                .always_vertical_scrollbar(true)
                .build(ui, || {
                    let mut ctx = &mut ctx.logview;

                    // If the user scrolled up, automatically disable following. Just don't do
                    // it on the very first frame in which following was turned on.
                    if !following_changed && ctx.following && ui.scroll_y() != ui.scroll_max_y() {
                        ctx.following = false;
                    }

                    if update_filter {
                        pool.update_filter();
                        ctx.filter_count = None;
                        ctx.cached_lines.clear();
                        if !ctx.following {
                            // TODO: here, we could try to keep the same position after
                            // filter change.
                            ui.set_scroll_y(0.0);
                        }
                    }

                    // Refresh filter count every second or so, unless
                    // it's very fast to do so.
                    if pool.fast_filter_count() || ctx.last_filter_count.elapsed().as_secs() >= 1 {
                        ctx.filter_count = None;
                    }
                    let num_lines = match ctx.filter_count {
                        None => {
                            let nl = pool.filter_count();
                            ctx.filter_count = Some(nl);
                            ctx.last_filter_count = Instant::now();
                            nl
                        }
                        Some(nl) => nl,
                    };

                    let mut kv_popup = false;

                    // Create a column grid. Imgui doesn't have helpers for configuring
                    // it only the first time (and then let the user resize it), so we
                    // need to handle that manually.
                    ui.columns(5, im_str!("##col"), true);
                    if !ctx.configured_columns {
                        ui.set_column_width(0, 50.0); // Frame number
                        ui.set_column_width(1, 50.0); // Log Level
                        ui.set_column_width(2, 140.0); // Module
                        ui.set_column_width(3, 260.0); // Message
                        ctx.configured_columns = true;
                    }

                    // Use a clipper to go through the grid and only draw visibile lines.
                    ImGuiListClipper::new(num_lines)
                        .items_height(ui.text_line_height())
                        .build(|start, end| {
                            let lines_iter = if ctx.following {
                                // If we're in following mode, just cheat and always load from the
                                // tail. We need to store the lines for borrowing rules, but let's always
                                // invalidate it right away as we don't need to cache it: the logpool
                                // query is very fast.
                                ctx.cached_lines = pool.filter_last((end - start + 1) as usize);
                                ctx.cached_start_line = usize::max_value();
                                ctx.cached_lines.iter()
                            } else {
                                // Normal mode, we use a local cache to avoid querying the logpool too much.
                                // See if the lines that we need to draw are already in the cache
                                if ctx.cached_start_line > start as usize
                                    || ctx.cached_start_line + ctx.cached_lines.len() < end as usize
                                {
                                    // Extract some additional lines from the pool, so that we don't have
                                    // to query it anytime we change position a little bit.
                                    let (start, end) = (
                                        (start as usize).saturating_sub(64),
                                        (end as usize).saturating_add(64),
                                    );
                                    ctx.cached_lines = pool.filter_get(start as u32, end as u32);
                                    ctx.cached_start_line = start;
                                }

                                let first = start as usize - ctx.cached_start_line;
                                let last = (first + (end - start + 1) as usize)
                                    .min(ctx.cached_lines.len());

                                ctx.cached_lines[first..last].iter()
                            };

                            // Now go through the loglines and draw them.
                            for v in lines_iter {
                                ui.text_colored(COLOR_FRAME, im_str!("[{}]", v.frame));
                                ui.next_column();
                                ui.text_colored(
                                    LOG_LEVEL_COLOR[v.level as usize],
                                    im_str!("{}", LOG_LEVEL_SHORT_NAMES[v.level as usize]),
                                );
                                ui.next_column();
                                ui.text_colored(COLOR_MODULE, &pool.modules[v.module as usize]);
                                ui.next_column();
                                ui.text(im_str!("{:80}", v.msg));
                                ui.next_column();
                                ui.text(im_str!("{}", v.kv.replace("\t", " ")));
                                if !ctx.following && ui.is_item_clicked(MouseButton::Left) {
                                    ctx.selected = v.kv.clone();
                                    kv_popup = true;
                                }
                                ui.next_column();
                            }
                        });

                    if !ctx.following {
                        if kv_popup {
                            ui.open_popup(im_str!("##kv"));
                        }
                        ui.popup(im_str!("##kv"), || {
                            ChildWindow::new(im_str!("##child"))
                                .size([300.0, 200.0])
                                .horizontal_scrollbar(true)
                                .build(&ui, || {
                                    ui.columns(2, im_str!("##col"), true);
                                    if kv_popup {
                                        // only when jsut opened, set column widths
                                        ui.set_column_width(0, 50.0);
                                        ui.set_column_width(1, 250.0);
                                    }
                                    ui.text(im_str!("Key"));
                                    ui.next_column();
                                    ui.text(im_str!("Value"));
                                    ui.next_column();
                                    ui.separator();
                                    for (n, kv) in ctx.selected.split('\t').enumerate() {
                                        let mut kv = kv.splitn(2, "=");
                                        let k = kv.next().unwrap();
                                        let v = kv.next().unwrap_or("");

                                        ui.text(im_str!("{}", k));
                                        ui.next_column();

                                        let mut buf = im_str!("{}", v).to_owned();
                                        if v.len() > 32 {
                                            ui.input_text_multiline(
                                                &im_str!("##v{}", n),
                                                &mut buf,
                                                [240.0, 0.0],
                                            )
                                            .auto_select_all(true)
                                            .read_only(true)
                                            .build();
                                        } else {
                                            ui.set_next_item_width(240.0);
                                            ui.input_text(&im_str!("##v{}", n), &mut buf)
                                                .auto_select_all(true)
                                                .read_only(true)
                                                .build();
                                        }
                                        ui.next_column();
                                    }
                                })
                        });
                    }
                    if ctx.following {
                        ui.set_scroll_here_y();
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::{new_debugger_logger, LogPool};
    use slog::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_logpool() {
        let (logger, pool) = new_debugger_logger();

        info!(logger, "test"; "a" => "b");
        warn!(logger, "test2"; "a" => "b");

        let mut pool = pool.lock().unwrap();
        let line = pool.last();
        assert_eq!(line.msg, "test2");
    }

    #[test]
    fn test_logpool_filter() {
        let (logger, pool) = new_debugger_logger();

        info!(logger, "test info"; "a" => "b");
        warn!(logger, "test warn first"; "a" => "b");
        info!(logger, "test info"; "a" => "b");
        warn!(logger, "test warn second"; "a" => "b");
        error!(logger, "test error 1"; "a" => "b");
        warn!(logger, "test warn third"; "a" => "b");
        error!(logger, "test error 2"; "a" => "b");
        info!(logger, "test info"; "a" => "b");

        let mut pool = pool.lock().unwrap();
        pool.filter.min_level = Some(Level::Warning.as_usize() as u8);
        pool.filter.max_level = Some(Level::Warning.as_usize() as u8);
        pool.update_filter();

        assert_eq!(pool.filter_count(), 3);
        let lines = pool.filter_get(0, 2);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].msg, "test warn first");
        assert_eq!(lines[1].msg, "test warn second");
        assert_eq!(lines[2].msg, "test warn third");

        let lines = pool.filter_get(2, 2);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].msg, "test warn third");

        let lines = pool.filter_get(2, 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].msg, "test warn third");

        let lines = pool.filter_get(7, 10);
        assert_eq!(lines.len(), 0);

        pool.filter.text = Some("second".to_string());
        pool.update_filter();

        assert_eq!(pool.filter_count(), 1);
        let lines = pool.filter_get(0, 2);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].msg, "test warn second");

        pool.filter.text = Some("SeCoNd".to_string());
        pool.update_filter();

        assert_eq!(pool.filter_count(), 1);
        let lines = pool.filter_get(0, 2);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].msg, "test warn second");
    }
}

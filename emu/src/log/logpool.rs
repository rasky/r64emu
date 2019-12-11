use rusqlite::{params, Connection, Result, Row, NO_PARAMS};
use slog;
use slog::{o, Drain, Logger};

use super::{
    LogDrain, LogPrinter, LogRecordPrinter, Record, ThreadSafeTimestampFn, KEY_FRAME, KEY_PC,
    KEY_SUBSYSTEM,
};

use std::collections::hash_map::{Entry, HashMap};
use std::convert::TryInto;
use std::fmt;
use std::io;
use std::io::Write;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
pub struct LogLine {
    pub(crate) level: u8,
    pub(crate) frame: u32,
    pub(crate) module: u8,
    pub(crate) location: Option<(String, u64)>,
    pub(crate) msg: String,
    pub(crate) kv: String,
}

impl LogLine {
    fn from_row(row: &Row) -> Result<LogLine> {
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

/// LogView is a view into the logpool that allwos to extract/analyze the loglines
/// possibly using filters.
/// Create a LogView, use [`LogPool::new_view()`](fn.LogPool.new_view.html). The view
/// keeps an (implicit) counting reference to the LogPool, so it continues to work
/// even if LogPool is dropped. The underlying log buffer memory
/// will not be released until all views (and the LogPool itself) is destroyed.
pub struct LogView {
    conn: Connection,
    table: String,

    min_level: Option<slog::Level>, // Minimum logging level to be displayed
    max_level: Option<slog::Level>, // Maximum logging level to be displayed
    min_frame: Option<u32>,         // Minimum frame to be displayed
    max_frame: Option<u32>,         // Maximum frame to be displayed
    modules: Option<Vec<u8>>,       // List of modules that must be displayed
    text: Option<String>,           // Full text search in msg+kv

    changed: bool,
    last_count: usize,
    last_count_rowid: i64,
}

impl LogView {
    fn new(conn: Connection, table: &str) -> LogView {
        LogView {
            conn,
            table: table.to_owned(),
            min_level: None,
            max_level: None,
            min_frame: None,
            max_frame: None,
            modules: None,
            text: None,
            changed: true,
            last_count: 0,
            last_count_rowid: 0,
        }
    }

    pub fn filter_min_level(&self) -> Option<slog::Level> {
        self.min_level
    }
    pub fn filter_max_level(&self) -> Option<slog::Level> {
        self.max_level
    }
    pub fn filter_min_frame(&self) -> Option<u32> {
        self.min_frame
    }
    pub fn filter_max_frame(&self) -> Option<u32> {
        self.max_frame
    }
    pub fn filter_modules(&self) -> Option<&Vec<u8>> {
        self.modules.as_ref()
    }
    pub fn filter_text(&self) -> Option<&String> {
        self.text.as_ref()
    }
    pub fn set_filter_min_level(&mut self, level: Option<slog::Level>) {
        self.min_level = level;
        self.changed = true;
    }
    pub fn set_filter_max_level(&mut self, level: Option<slog::Level>) {
        self.max_level = level;
        self.changed = true;
    }
    pub fn set_filter_min_frame(&mut self, frame: Option<u32>) {
        self.min_frame = frame;
        self.changed = true;
    }
    pub fn set_filter_max_frame(&mut self, frame: Option<u32>) {
        self.max_frame = frame;
        self.changed = true;
    }
    pub fn set_filter_modules(&mut self, modules: Option<Vec<u8>>) {
        self.modules = modules;
        self.changed = true;
    }
    pub fn set_filter_text(&mut self, text: Option<&str>) {
        self.text = text.map(|s| s.to_owned());
        self.changed = true;
    }

    fn update_filter(&mut self) {
        if !self.changed {
            return;
        }
        self.changed = false;
        self.conn
            .execute(&format!("DROP VIEW IF EXISTS {}", self.table), NO_PARAMS)
            .unwrap();
        let mut view = format!(
            "CREATE VIEW {} AS SELECT
            id, level, frame, module, msg, kv FROM log ",
            self.table
        );

        // Now build all the filters in the WHERE clause.
        // Unfortunately, SQLite does not support params in view
        // statements, so we must manually interpolate the arguments.
        let has_level = self.min_level.is_some() || self.max_level.is_some();
        let has_module = self.modules.is_some();
        let has_frame = self.min_frame.is_some() || self.max_frame.is_some();

        let mut prefix = "WHERE ";

        if let Some(ref modules) = self.modules {
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

        if let Some(min_frame) = self.min_frame {
            view = view + prefix + &format!("frame >= {} ", min_frame);
            prefix = "AND ";
        }
        if let Some(max_frame) = self.max_frame {
            view = view + prefix + &format!("frame <= {} ", max_frame);
            prefix = "AND ";
        }

        if let Some(min_level) = self.min_level {
            view = view + prefix + &format!("level >= {} ", min_level.as_usize() as u8);
            prefix = "AND ";
        }
        if let Some(max_level) = self.max_level {
            view = view + prefix + &format!("level <= {} ", max_level.as_usize() as u8);
            prefix = "AND ";
        }

        if let Some(ref text) = self.text {
            let text = text.replace("'", "''");
            view = view + prefix + &format!("(msg LIKE '%{}%' OR kv LIKE '%{}%')", text, text);
        }
        self.conn.execute(&view, NO_PARAMS).unwrap();
        self.last_count_rowid = 0;
        self.last_count = 0;
    }

    // Returns loglines from this view of the pool; [first, last] is the inclusive range of
    // loglines indices that specify which portion of the filtered lines will be extracted.
    pub fn get(&mut self, first: u32, last: u32) -> Vec<LogLine> {
        self.update_filter();
        let mut stmt = self
            .conn
            .prepare_cached(&format!("SELECT * FROM {} ORDER BY id LIMIT ? OFFSET ?", self.table))
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

    // Returns the last n lines in this view of the pool, useful for "following" the loglines.
    // This is semantically equivalent to `fiter_get(filter_count()-n, filter_count())`
    // but it's much faster.
    pub fn last(&mut self, n: usize) -> Vec<LogLine> {
        self.update_filter();
        let mut stmt = self
            .conn
            .prepare_cached(&format!("SELECT * FROM {} ORDER by id DESC LIMIT ?", self.table))
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

    /// Returns the number of loglines in this view.
    /// Note that this function might be slow depending on the exact filters being
    /// activated and the size of the pool. Use (`is_fast_count()`)[fn.LogPool.is_fast_count.html]
    /// to estimate the speed of this operation.
    pub fn count(&mut self) -> usize {
        self.update_filter();
        // Since calculating the total count can be slow when the filter contains,
        // many elements, this function works incrementally: it caches the last
        // computed count, and updates it counting only the new loglines
        // arrived since the previous call.
        let mut stmt = self
            .conn
            .prepare_cached(&format!("SELECT COUNT(*) FROM {} WHERE id > ?", self.table))
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

    /// Returns true if [`fn.LogView.count.html`]() is supposed to be "fast".
    /// Currently, this function returns true only when no filter is activated.
    pub fn is_fast_count(&self) -> bool {
        self.text.is_none() && false
    }
}

/// LogPool is a in-memory log buffer that collects logs from slog and stores them
/// for efficient filtering/searching and displaying. It can be used to store loglines
/// and analyzes them procedurally. For instance, it's used by emu::dbg to
/// collect all the logs and display them in a log view.
///
/// Use [`new_pool_logger()`](fn.new_pool_logger.html) to create a `LogPool` and
/// a `slog::Logger` connected to it.
pub struct LogPool {
    pub(crate) modules: Vec<String>,
    modidx: HashMap<String, u8>,
    dburl: String,
    conn: Connection,
    num_lines: usize,
    send_analyze: mpsc::Sender<bool>,
    num_views: usize,
}

/// LogPoolPtr is a pointer that wraps [`LogPool`](struct.LogPool.html) for safe
/// threading usage.
pub type LogPoolPtr = Arc<Mutex<Box<LogPool>>>;

impl LogPool {
    fn new() -> LogPoolPtr {
        let dburl = format!("file:{}?mode=memory&cache=shared", "logpool1");
        let conn = Connection::open(&dburl).unwrap();
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

        let (sender, receiver) = mpsc::channel();
        let dburl2 = dburl.clone();
        std::thread::spawn(move || {
            let conn = Connection::open(&dburl2).unwrap();
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

        let pool = LogPool {
            modidx: HashMap::new(),
            modules: Vec::new(),
            dburl,
            conn: conn,
            num_lines: 0,
            num_views: 0,
            send_analyze: sender,
        };
        Arc::new(Mutex::new(Box::new(pool)))
    }

    pub fn log(&mut self, line: LogLine) {
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

    /// Create a new [`LogView`](struct.LogView.html) that allows to filter and extract
    /// loglines from this pool. The `LogView` will implicitly share this logpool, so
    /// it will continue to work even if LogPool is dropped.
    pub fn new_view(&mut self) -> LogView {
        self.num_views += 1; // just used to create a unique ID
        LogView::new(
            Connection::open(&self.dburl).unwrap(),
            &format!("logpool_view_{}", self.num_views),
        )
    }
}

struct PoolRecordPrinter {
    pool: LogPoolPtr,

    line: LogLine,
    pc: Option<u64>,
    sub: Option<String>,
    kv: Vec<u8>,
}

impl LogRecordPrinter for PoolRecordPrinter {
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
            pool.modules.push(tag.to_string());
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

struct PoolPrinter {
    pool: LogPoolPtr,
}

impl PoolPrinter {
    fn new(pool: LogPoolPtr) -> PoolPrinter {
        PoolPrinter { pool }
    }
}

impl LogPrinter for PoolPrinter {
    type RecordPrinter = PoolRecordPrinter;

    fn with_record<F>(&self, _record: &Record, f: F) -> io::Result<()>
    where
        F: FnOnce(Self::RecordPrinter) -> io::Result<()>,
    {
        f(PoolRecordPrinter {
            pool: self.pool.clone(),
            line: LogLine::default(),
            kv: Vec::new(),
            pc: None,
            sub: None,
        })
    }
}

/// Create a `slog::Logger` whose output is piped into an in-memory buffer ([`LogPool`](struct.LogPool.html)).
pub fn new_pool_logger() -> (slog::Logger, LogPoolPtr) {
    let pool = LogPool::new();
    let printer = PoolPrinter::new(pool.clone());
    let drain = LogDrain::new(printer).build().fuse();
    let logger = slog::Logger::root(drain, o!());
    (logger, pool)
}

#[cfg(test)]
mod tests {
    use super::{new_pool_logger, LogPool};
    use slog::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_logpool() {
        let (logger, pool) = new_pool_logger();

        info!(logger, "test"; "a" => "b");
        warn!(logger, "test2"; "a" => "b");

        let mut pool = pool.lock().unwrap();
        let line = pool.last();
        assert_eq!(line.msg, "test2");
    }

    #[test]
    fn test_logpool_filter() {
        let (logger, pool) = new_pool_logger();

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

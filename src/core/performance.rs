//! Bounded, asynchronous local timings. Never record document or request contents.
use std::{
    cell::Cell,
    fs::{File, OpenOptions},
    io::{self, Write},
    marker::PhantomData,
    path::Path,
    rc::Rc,
    sync::{
        Arc, OnceLock,
        atomic::AtomicBool,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: usize = 4 * 1024 * 1024;
static SINK: OnceLock<SyncSender<WriteCommand>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static DROPPED: AtomicU64 = AtomicU64::new(0);
thread_local! { static JOB: Cell<u64> = const { Cell::new(0) }; }

#[derive(serde::Serialize)]
struct Record {
    unix_ms: u128,
    version: &'static str,
    stage: &'static str,
    event: &'static str,
    span: u64,
    job: u64,
    duration_ms: f64,
    dropped_records: u64,
}

enum WriteCommand {
    Record(Record),
    Shutdown(mpsc::Sender<io::Result<()>>),
}

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Called once on desktop startup; the writer never blocks the controller.
pub fn initialize(path: &Path) -> io::Result<()> {
    if SINK.get().is_some() {
        return Ok(());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let (tx, rx) = mpsc::sync_channel(512);
    std::thread::Builder::new()
        .name("harmonica-perf-log".into())
        .spawn(move || {
            let _ = write_commands(file, rx, MAX_BYTES);
        })?;
    let _ = SINK.set(tx);
    elapsed("session", Duration::ZERO, 0);
    Ok(())
}

#[cfg(test)]
fn write_records(
    mut file: File,
    records: impl IntoIterator<Item = Record>,
    limit: usize,
) -> io::Result<()> {
    let mut bytes = 0;
    for record in records {
        if !write_record(&mut file, record, &mut bytes, limit)? {
            break;
        }
    }
    Ok(())
}

fn write_record(file: &mut File, record: Record, bytes: &mut usize, limit: usize) -> io::Result<bool> {
    let mut line = serde_json::to_vec(&record)?;
    line.push(b'\n');
    if *bytes + line.len() + 64 > limit {
        file.write_all(b"{\"stage\":\"log_limit\",\"event\":\"recording_stopped\"}\n")?;
        return Ok(false);
    }
    file.write_all(&line)?;
    *bytes += line.len();
    Ok(true)
}

fn write_commands(
    mut file: File,
    commands: impl IntoIterator<Item = WriteCommand>,
    limit: usize,
) -> io::Result<()> {
    let mut bytes = 0;
    let mut stopped = false;
    for command in commands {
        match command {
            WriteCommand::Record(record) => {
                if stopped {
                    continue;
                }
                if !write_record(&mut file, record, &mut bytes, limit)? {
                    file.sync_all()?;
                    stopped = true;
                    continue;
                }
                // Keep a live log readable even if the app remains open for hours.
                file.sync_data()?;
            }
            WriteCommand::Shutdown(reply) => {
                let result = file.sync_all();
                let _ = reply.send(result);
                return Ok(());
            }
        }
    }
    file.sync_all()
}

/// Drain all queued records after the UI and heartbeat have stopped.
pub fn shutdown() -> io::Result<()> {
    let Some(sink) = SINK.get() else {
        return Ok(());
    };
    shutdown_sink(sink, Duration::from_secs(2))
}

fn shutdown_sink(sink: &SyncSender<WriteCommand>, timeout: Duration) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    let until = Instant::now() + timeout;
    loop {
        match sink.try_send(WriteCommand::Shutdown(tx.clone())) {
            Ok(()) => break,
            Err(mpsc::TrySendError::Full(_)) if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(mpsc::TrySendError::Full(_)) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "性能日志队列未能及时排空",
                ));
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "性能日志写入线程已退出",
                ));
            }
        }
    }
    drop(tx);
    rx.recv_timeout(until.saturating_duration_since(Instant::now()))
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => {
                io::Error::new(io::ErrorKind::TimedOut, "性能日志未能及时确认落盘")
            }
            mpsc::RecvTimeoutError::Disconnected => {
                io::Error::new(io::ErrorKind::BrokenPipe, "性能日志写入线程未确认落盘")
            }
        })?
}

fn emit(stage: &'static str, event: &'static str, span: u64, job: u64, duration: Duration) {
    let Some(sink) = SINK.get() else {
        return;
    };
    let record = Record {
        unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        version: env!("CARGO_PKG_VERSION"),
        stage,
        event,
        span,
        job,
        duration_ms: duration.as_secs_f64() * 1000.,
        dropped_records: DROPPED.load(Ordering::Relaxed),
    };
    if sink.try_send(WriteCommand::Record(record)).is_err() {
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn elapsed(stage: &'static str, duration: Duration, job: u64) {
    emit(stage, "elapsed", next_id(), job, duration);
}

/// A separate thread distinguishes a delayed UI dispatcher from a process-wide pause.
pub struct ProcessHeartbeat {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

pub fn start_process_heartbeat() -> io::Result<ProcessHeartbeat> {
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name("harmonica-perf-heartbeat".into())
        .spawn(move || {
            let mut last = Instant::now();
            while !worker_stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
                let gap = last.elapsed();
                last = Instant::now();
                if gap >= Duration::from_millis(500) {
                    elapsed("process.heartbeat_gap", gap, 0);
                }
            }
        })?;
    Ok(ProcessHeartbeat {
        stop,
        thread: Some(thread),
    })
}

impl Drop for ProcessHeartbeat {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct Span {
    stage: &'static str,
    id: u64,
    job: u64,
    start: Instant,
}
impl Span {
    pub fn new(stage: &'static str) -> Self {
        let span = Self {
            stage,
            id: next_id(),
            job: JOB.get(),
            start: Instant::now(),
        };
        emit(stage, "begin", span.id, span.job, Duration::ZERO);
        span
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        emit(self.stage, "end", self.id, self.job, self.start.elapsed());
    }
}

pub struct JobContext {
    previous: u64,
    _local: PhantomData<Rc<()>>,
}
pub fn job_context(job: u64) -> JobContext {
    JobContext {
        previous: JOB.replace(job),
        _local: PhantomData,
    }
}
impl Drop for JobContext {
    fn drop(&mut self) {
        JOB.set(self.previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_log_is_json_and_does_not_overwrite_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("performance.jsonl");
        let records = (0..100).map(|id| Record {
            unix_ms: 0,
            version: "test",
            stage: "midi.parse",
            event: "end",
            span: id,
            job: 1,
            duration_ms: 10.,
            dropped_records: 0,
        });
        write_records(File::create(&path).unwrap(), records, 1024).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.len() <= 1024);
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(rows.last().unwrap()["stage"], "log_limit");
        assert!(initialize(&path).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), text);
    }
    #[test]
    fn nested_job_context_is_restored() {
        let original = JOB.get();
        {
            let _outer = job_context(17);
            {
                let _inner = job_context(25);
                assert_eq!(JOB.get(), 25);
            }
            assert_eq!(JOB.get(), 17);
        }
        assert_eq!(JOB.get(), original);
    }

    #[test]
    fn shutdown_confirms_queued_records_are_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timing.jsonl");
        let (tx, rx) = mpsc::sync_channel(2);
        let file = File::create(&path).unwrap();
        let writer = std::thread::spawn(move || write_commands(file, rx, MAX_BYTES));
        tx.send(WriteCommand::Record(Record {
            unix_ms: 1,
            version: "test",
            stage: "ui.timer.fire_late.visible",
            event: "elapsed",
            span: 1,
            job: 0,
            duration_ms: 500.,
            dropped_records: 0,
        }))
        .unwrap();
        let (reply_tx, reply_rx) = mpsc::channel();
        tx.send(WriteCommand::Shutdown(reply_tx)).unwrap();
        reply_rx.recv().unwrap().unwrap();
        writer.join().unwrap().unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("ui.timer.fire_late.visible"));
    }

    #[test]
    fn shutdown_does_not_wait_forever_on_a_stuck_writer() {
        let (tx, _rx) = mpsc::sync_channel(0);
        let started = Instant::now();
        let error = shutdown_sink(&tx, Duration::from_millis(30)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}

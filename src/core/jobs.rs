//! Workers own immutable snapshots. Only the controller's poll installs results.
use crate::project::Project;
use anyhow::{Result, bail};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowUp {
    None,
    Play,
    Game,
    Arm,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Load,
    Convert,
    Export,
    Library,
}
pub struct Job<T> {
    pub number: u64,
    pub revision: u64,
    pub document: u64,
    pub kind: JobKind,
    pub follow_up: FollowUp,
    pub cancel: Arc<AtomicBool>,
    pub timing_id: u64,
    receiver: mpsc::Receiver<(Result<T>, Instant)>,
}
pub struct Completed<T> {
    pub job: Job<T>,
    pub result: Result<T>,
}
pub struct JobRunner<T> {
    pub current: Option<Job<T>>,
    next: u64,
}
impl<T: Send + 'static> Default for JobRunner<T> {
    fn default() -> Self {
        Self {
            current: None,
            next: 0,
        }
    }
}
impl<T: Send + 'static> JobRunner<T> {
    pub fn start<F>(
        &mut self,
        kind: JobKind,
        revision: u64,
        document: u64,
        follow_up: FollowUp,
        work: F,
    ) -> Result<()>
    where
        F: FnOnce(&AtomicBool) -> Result<T> + Send + 'static,
    {
        if self.current.is_some() {
            bail!("曲谱正在准备，请稍候。")
        }
        self.next += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, rx) = mpsc::channel();
        let timing_id = crate::performance::next_id();
        let queued = Instant::now();
        thread::Builder::new()
            .name(format!("harmonica-{kind:?}"))
            .spawn(move || {
                let _context = crate::performance::job_context(timing_id);
                crate::performance::elapsed("job.queue", queued.elapsed(), timing_id);
                let timing = crate::performance::Span::new(match kind {
                    JobKind::Load => "job.load",
                    JobKind::Convert => "job.convert",
                    JobKind::Export => "job.export",
                    JobKind::Library => "job.library",
                });
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&flag)))
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("后台任务意外结束")));
                drop(timing);
                let _ = tx.send((result, Instant::now()));
            })?;
        self.current = Some(Job {
            number: self.next,
            revision,
            document,
            kind,
            follow_up,
            cancel,
            timing_id,
            receiver: rx,
        });
        Ok(())
    }
    pub fn busy(&self) -> bool {
        self.current.is_some()
    }
    pub fn cancel(&mut self) {
        if let Some(j) = &mut self.current {
            j.follow_up = FollowUp::None;
            j.cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn clear_follow_up(&mut self, follow: FollowUp) {
        if let Some(j) = &mut self.current {
            if j.follow_up == follow {
                j.follow_up = FollowUp::None
            }
        }
    }
    pub fn take_completed(&mut self) -> Option<Completed<T>> {
        let result = match self.current.as_ref()?.receiver.try_recv() {
            Ok((r, finished)) => {
                crate::performance::elapsed(
                    "job.result_wait",
                    finished.elapsed(),
                    self.current.as_ref()?.timing_id,
                );
                r
            }
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(_) => Err(anyhow::anyhow!("后台任务未返回结果")),
        };
        Some(Completed {
            job: self.current.take().unwrap(),
            result,
        })
    }
}
impl<T> Drop for JobRunner<T> {
    fn drop(&mut self) {
        if let Some(j) = &self.current {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveKind {
    Auto,
    Manual,
    Preserve,
    Close,
}
#[derive(Clone, Debug)]
pub struct SaveRequest {
    pub number: u64,
    pub document: u64,
    pub revision: u64,
    pub project: Option<Project>,
    pub paths: Vec<PathBuf>,
    pub kind: SaveKind,
}
pub struct SaveCompletion {
    pub request: SaveRequest,
    pub error: Option<anyhow::Error>,
}
pub type SaveWriter = Arc<dyn Fn(&std::path::Path, &Project) -> Result<()> + Send + Sync>;
pub struct SaveQueue {
    next: u64,
    pending: VecDeque<SaveRequest>,
    active: Option<(SaveRequest, mpsc::Receiver<Result<()>>)>,
    writer: SaveWriter,
}
impl Default for SaveQueue {
    fn default() -> Self {
        Self::with_writer(Arc::new(crate::project::save_project))
    }
}
impl SaveQueue {
    pub const MAX_PENDING: usize = 16;
    pub fn with_writer(writer: SaveWriter) -> Self {
        Self {
            next: 0,
            pending: VecDeque::new(),
            active: None,
            writer,
        }
    }
    pub fn busy(&self) -> bool {
        self.active.is_some() || !self.pending.is_empty()
    }
    pub fn submit(
        &mut self,
        document: u64,
        revision: u64,
        project: Option<Project>,
        paths: Vec<PathBuf>,
        kind: SaveKind,
    ) -> Result<u64> {
        let coalesce = kind == SaveKind::Auto
            && self.pending.back().is_some_and(|r| {
                r.kind == SaveKind::Auto && r.document == document && r.paths == paths
            });
        if !coalesce && self.pending.len() >= Self::MAX_PENDING {
            bail!("等待保存的请求过多，请稍后再试。")
        }
        self.next += 1;
        let r = SaveRequest {
            number: self.next,
            document,
            revision,
            project,
            paths,
            kind,
        };
        if coalesce {
            self.pending.pop_back();
        }
        self.pending.push_back(r);
        self.start_next();
        Ok(self.next)
    }
    fn start_next(&mut self) {
        if self.active.is_some() {
            return;
        }
        let Some(r) = self.pending.pop_front() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let copy = r.clone();
        let writer = self.writer.clone();
        let fail_tx = tx.clone();
        let launched = thread::Builder::new()
            .name("harmonica-save".into())
            .spawn(move || {
                let _timing = crate::performance::Span::new("save.write");
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Some(p) = &copy.project {
                        for path in &copy.paths {
                            writer(path, p)?;
                        }
                    }
                    Ok(())
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("保存任务意外结束")));
                let _ = tx.send(result);
            });
        if let Err(e) = launched {
            let _ = fail_tx.send(Err(e.into()));
        }
        self.active = Some((r, rx));
    }
    pub fn take_completed(&mut self) -> Option<SaveCompletion> {
        let result = match self.active.as_ref()?.1.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(_) => Err(anyhow::anyhow!("保存任务未返回结果")),
        };
        let (r, _) = self.active.take().unwrap();
        self.start_next();
        Some(SaveCompletion {
            request: r,
            error: result.err(),
        })
    }
}

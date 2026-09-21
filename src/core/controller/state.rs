use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Ready,
    Playing,
    Paused,
    Ended,
}
#[derive(Clone, Debug)]
pub enum ControllerEvent {
    Changed,
    Document,
    Parts,
    Result,
    Library,
    Position,
    Error(String),
    CloseReady,
}
pub struct AppState {
    pub project: Option<Project>,
    pub project_path: Option<PathBuf>,
    pub auto_project_path: Option<PathBuf>,
    pub source: Option<PathBuf>,
    pub parts: Parts,
    pub names: TrackNames,
    pub keys: Vec<(usize, u8)>,
    pub recommendations: std::collections::BTreeMap<(usize, u8), f64>,
    pub selected_part: Option<(usize, u8)>,
    pub result: Option<ExportResult>,
    pub revision: u64,
    pub document_id: u64,
    pub saved_revision: u64,
    pub autosave_revision: Option<u64>,
    pub export_revision: Option<u64>,
    pub save_error: String,
    pub transition: Option<String>,
    pub score_duration: f64,
    pub transport: Transport,
    pub logical_seek: f64,
    pub position: f64,
    pub preview_duration: f64,
    pub position_label: String,
    pub show_cursor: bool,
    pub message: String,
    pub library_revision: u64,
    pub library_root: Option<PathBuf>,
    pub library_error: String,
    pub closed: bool,
}
impl Default for AppState {
    fn default() -> Self {
        Self {
            project: None,
            project_path: None,
            auto_project_path: None,
            source: None,
            parts: Parts::default(),
            names: TrackNames::default(),
            keys: vec![],
            recommendations: Default::default(),
            selected_part: None,
            result: None,
            revision: 0,
            document_id: 0,
            saved_revision: 0,
            autosave_revision: None,
            export_revision: None,
            save_error: String::new(),
            transition: None,
            score_duration: 0.,
            transport: Transport::Ready,
            logical_seek: 0.,
            position: 0.,
            preview_duration: 0.,
            position_label: "未播放".into(),
            show_cursor: false,
            message: "从曲库选歌，或打开 MIDI。".into(),
            library_revision: 0,
            library_root: None,
            library_error: String::new(),
            closed: false,
        }
    }
}
impl AppState {
    pub fn project_dirty(&self) -> bool {
        self.project.is_some() && self.revision != self.saved_revision
    }
    pub fn export_dirty(&self) -> bool {
        self.project.is_some()
            && (self.result.is_none() || Some(self.revision) != self.export_revision)
    }
    pub fn has_notes(&self) -> bool {
        self.project.as_ref().is_some_and(|p| !p.notes.is_empty())
    }
    pub fn set_project(&mut self, project: Option<Project>, saved: bool, new_document: bool) {
        self.project = project;
        self.revision += 1;
        if new_document {
            self.document_id += 1;
            self.autosave_revision = None;
            self.save_error.clear();
        }
        self.score_duration = self
            .project
            .as_ref()
            .map(|p| p.notes.iter().map(|n| n.end).fold(0., f64::max))
            .unwrap_or(0.);
        if saved {
            self.saved_revision = self.revision;
        }
    }
}
#[derive(Clone, Debug)]
pub struct Capabilities {
    pub can_open: bool,
    pub can_save: bool,
    pub can_export: bool,
    pub can_play: bool,
    pub can_edit: bool,
    pub can_convert: bool,
    pub current_export: bool,
}

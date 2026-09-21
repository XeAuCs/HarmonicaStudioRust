use super::*;

impl AppController {
    pub fn start_remote(&mut self, port: u16) -> Result<String> {
        ensure!(!self.state.closed, "应用已关闭。");
        if self.remote.is_none() {
            self.remote = Some(RemoteServer::start("0.0.0.0", port)?);
        }
        let state = self.remote_state();
        let remote = self.remote.as_ref().unwrap();
        remote.publish(&state, &self.score_cache)?;
        Ok(remote.url())
    }
    pub fn stop_remote(&mut self) {
        if let Some(mut remote) = self.remote.take() {
            remote.stop();
        }
    }
    pub fn remote_url(&self) -> Option<String> {
        self.remote.as_ref().map(RemoteServer::url)
    }
    pub fn remote_addresses(&self) -> Vec<(String, String)> {
        self.remote
            .as_ref()
            .map(RemoteServer::address_options)
            .unwrap_or_default()
    }
    pub fn remote_url_for(&self, address: &str) -> Option<String> {
        self.remote
            .as_ref()
            .and_then(|remote| remote.url_for(address))
    }
    pub fn remote_client_connected(&self) -> bool {
        self.remote
            .as_ref()
            .is_some_and(RemoteServer::client_connected)
    }
    pub(super) fn refresh_score_cache(&mut self) {
        let key = (
            self.state.document_id,
            self.state.revision,
            self.state.export_revision,
            self.preferences.skip_long_rests,
        );
        if self.score_key == Some(key) {
            return;
        }
        let s = &self.state;
        let notes = s
            .result
            .as_ref()
            .filter(|_| !s.export_dirty())
            .map(|r| r.actual.as_slice())
            .unwrap_or_else(|| {
                s.project
                    .as_ref()
                    .map(|p| p.notes.as_slice())
                    .unwrap_or(&[])
            });
        let compact: Vec<Value> = notes
            .iter()
            .map(|n| json!([n.start, n.end, n.pitch]))
            .collect();
        let mut low = notes.iter().map(|n| n.pitch).min().unwrap_or(62) - 2;
        let mut high = notes.iter().map(|n| n.pitch).max().unwrap_or(70) + 2;
        if high - low < 12 {
            let center = (low + high) / 2;
            low = center - 6;
            high = center + 6;
        }
        let payload = json!({"notes":compact,"duration":notes.iter().map(|n|n.end).fold(0.,f64::max),"low":low,"high":high});
        let id = if notes.is_empty() {
            "empty".into()
        } else {
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&payload).unwrap())
            )[..24]
                .to_string()
        };
        let mut score = payload;
        score["id"] = json!(id);
        self.score_cache = score;
        self.score_key = Some(key);
    }
    pub fn remote_snapshot(&mut self) -> (Value, Value) {
        let state = self.remote_state();
        (state, self.score_cache.clone())
    }
    pub(super) fn remote_state(&mut self) -> Value {
        self.refresh_score_cache();
        let s = &self.state;
        let id = &self.score_cache["id"];
        let library:Vec<_>=self.library.iter().map(|e|json!({"id":song_id(&e.path),"title":e.title,"duration_seconds":e.duration_seconds})).collect();
        let can_play = self.capabilities().can_play;
        let mut game = serde_json::to_value(self.player.status()).unwrap();
        game["available"] = json!(can_play);
        let palette = match self.preferences.theme.as_str() {
            "forest" => {
                json!({"bg":"#EDF0EB","surface":"#F8FAF5","ink":"#25322E","muted":"#616F66","line":"#C4CEC4","grid":"#DFE6DC","accent":"#386C5F","accent_soft":"#DDE9DE","accent_ink":"#F8FCF6","selection":"#CDDFD0","playhead":"#A1683E"})
            }
            "blue" => {
                json!({"bg":"#EEF0F2","surface":"#FAFAFC","ink":"#29313D","muted":"#656E7D","line":"#C8CDD6","grid":"#E1E5EC","accent":"#4A6084","accent_soft":"#E0E6EF","accent_ink":"#FBFCFF","selection":"#D1DCEE","playhead":"#AC6842"})
            }
            "plum" => {
                json!({"bg":"#F2EEEF","surface":"#FCF9FA","ink":"#352C34","muted":"#756873","line":"#D0C5CD","grid":"#E9DFE5","accent":"#805369","accent_soft":"#ECDDDF","accent_ink":"#FFFAFC","selection":"#E2CBD5","playhead":"#AF684A"})
            }
            _ => {
                json!({"bg":"#F4F1EA","surface":"#FCFAF5","ink":"#292720","muted":"#6E695F","line":"#CFC8BB","grid":"#E5DED2","accent":"#9F4937","accent_soft":"#EFE0D8","accent_ink":"#FFFAF4","selection":"#E6D8CC","playhead":"#B15339"})
            }
        };
        let state = json!({"title":s.project.as_ref().map(|p|p.title.as_str()).unwrap_or("选择一首曲谱"),"song_id":s.source.as_deref().map(song_id),"transport":s.transport,"position":s.position,"duration":if s.preview_duration>0.{s.preview_duration}else{s.score_duration},"busy":self.busy(),"status":s.message,"theme":self.preferences.theme,"palette":palette,"library":library,"can_play":can_play,"game":game,"score_id":id,"library_refreshing":self.library_refreshing(),"library_revision":s.library_revision,"library_error":if s.library_error.is_empty(){""}else{"曲库刷新失败，请检查电脑端状态。"},"saving":self.saving(),"transition":s.transition,"highlight":s.project.as_ref().and_then(|p|p.highlight),"start_from_highlight":self.preferences.start_from_highlight});
        state
    }
}

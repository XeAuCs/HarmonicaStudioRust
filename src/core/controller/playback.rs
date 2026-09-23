use super::*;

impl AppController {
    pub(super) fn load_preview(&mut self) -> Result<()> {
        let _timing = crate::performance::Span::new("preview.open_audio");
        let result = self.state.result.as_ref().context("请先生成试听")?;
        self.audio.load(&result.folder.join("试听.wav"))?;
        self.state.preview_duration = self.audio.duration();
        self.state.transport = Transport::Ready;
        self.state.position_label = "未播放".into();
        let logical = self.state.logical_seek;
        let manual = self.manual_seek;
        self.seek(logical)?;
        self.manual_seek = manual;
        Ok(())
    }
    pub(super) fn to_audio(&self, seconds: f64) -> f64 {
        self.state
            .result
            .as_ref()
            .filter(|_| !self.state.export_dirty())
            .map(|r| r.to_audio.map(seconds))
            .unwrap_or(seconds)
    }
    pub(super) fn to_score(&self, seconds: f64) -> f64 {
        self.state
            .result
            .as_ref()
            .filter(|_| !self.state.export_dirty())
            .map(|r| r.to_score.map(seconds))
            .unwrap_or(seconds)
    }
    pub fn seek(&mut self, seconds: f64) -> Result<()> {
        ensure!(seconds.is_finite(), "播放位置必须是有限数字。");
        let score = seconds.clamp(0., self.state.score_duration);
        let seconds = if self.state.preview_duration > 0. {
            self.to_audio(score)
        } else {
            score
        };
        self.seek_audio(seconds)
    }
    pub fn seek_audio(&mut self, seconds: f64) -> Result<()> {
        ensure!(seconds.is_finite(), "播放位置必须是有限数字。");
        let duration = if self.state.preview_duration > 0. {
            self.state.preview_duration
        } else {
            self.state.score_duration
        };
        let seconds = seconds.clamp(0., duration);
        if self.state.preview_duration > 0. {
            self.audio
                .seek(seconds, self.state.transport == Transport::Playing)?;
            self.state.position = self.audio.position()?;
            self.state.logical_seek = self.to_score(self.state.position);
        } else {
            self.state.position = seconds;
            self.state.logical_seek = seconds;
        }
        if self.state.transport != Transport::Playing && self.state.transport != Transport::Paused {
            self.state.transport = Transport::Ready;
            self.state.position_label = "未播放".into();
        }
        self.playback_clock.reset(
            self.state.position,
            self.state.transport == Transport::Playing,
        );
        self.last_audio_poll = Instant::now();
        self.manual_seek = true;
        self.state.show_cursor = true;
        self.events.push(ControllerEvent::Position);
        Ok(())
    }
    pub fn listen(&mut self) -> Result<()> {
        self.idle()?;
        ensure!(self.state.has_notes(), "请先生成或打开曲谱。");
        if self.state.export_dirty() {
            return self.begin_export(FollowUp::Play);
        }
        self.player.stop()?;
        if self.state.preview_duration <= 0. {
            self.load_preview()?;
        }
        if self.preferences.start_from_highlight
            && !self.manual_seek
            && matches!(self.state.transport, Transport::Ready | Transport::Ended)
        {
            if let Some(h) = self.state.project.as_ref().and_then(|p| p.highlight) {
                self.seek(h)?;
            }
        }
        let mut start = self.audio.position()?;
        if self.state.transport == Transport::Ended || start >= self.state.preview_duration - 0.01 {
            start = 0.;
            self.state.logical_seek = 0.;
        }
        self.audio.play(start)?;
        self.playback_clock.reset(start, true);
        self.last_audio_poll = Instant::now();
        self.state.transport = Transport::Playing;
        self.state.position_label = "试听中".into();
        self.state.position = start;
        self.state.show_cursor = true;
        self.manual_seek = false;
        self.status("正在试听…");
        Ok(())
    }
    pub fn toggle_preview(&mut self) -> Result<()> {
        if self.state.transport == Transport::Playing {
            self.pause()
        } else {
            self.listen()
        }
    }
    pub fn pause(&mut self) -> Result<()> {
        if self.state.transport == Transport::Playing {
            self.audio.pause()?;
            self.state.position = self.audio.position()?;
            self.state.logical_seek = self.to_score(self.state.position);
            self.state.transport = Transport::Paused;
            self.state.position_label = "已暂停".into();
            self.playback_clock.reset(self.state.position, false);
            self.status("试听已暂停。");
        }
        Ok(())
    }
    pub fn stop(&mut self) -> Result<()> {
        self.jobs.clear_follow_up(FollowUp::Play);
        if let Some(p) = &mut self.pending {
            if p.follow == FollowUp::Play {
                p.follow = FollowUp::None;
            }
        }
        self.audio.stop()?;
        self.playback_clock.reset(0., false);
        self.state.transport = Transport::Ready;
        self.state.position_label = "未播放".into();
        self.state.position = 0.;
        self.state.logical_seek = 0.;
        self.state.show_cursor = false;
        self.manual_seek = false;
        self.status("试听已停止。");
        Ok(())
    }
    pub fn game_play(&mut self, arm: bool) -> Result<()> {
        self.idle()?;
        ensure!(self.state.has_notes(), "请先选择歌曲。");
        if self.state.export_dirty() {
            return self.begin_export(if arm { FollowUp::Arm } else { FollowUp::Game });
        }
        self.stop()?;
        let result = self.state.result.as_ref().context("请先生成曲谱")?;
        let script = result.folder.join("演奏脚本.ahk");
        let start = if self.preferences.start_from_highlight {
            self.state
                .project
                .as_ref()
                .and_then(|p| p.highlight)
                .map(|h| {
                    self.to_audio(h).clamp(
                        0.,
                        (self.to_audio(self.state.score_duration) - 0.001).max(0.),
                    )
                })
                .unwrap_or(0.)
        } else {
            0.
        };
        if arm {
            self.player.start(&script, start)?;
        } else {
            self.player.play(&script, start)?;
        }
        self.status(if arm {
            "演奏器已就绪；切到口琴界面按 F6，F8 退出。"
        } else {
            "已发送演奏指令，请保持游戏口琴界面在前台。"
        });
        Ok(())
    }
    /// Includes queued starts and a helper that is still releasing its keys.
    pub fn game_active(&self) -> bool {
        self.player.active()
            || self
                .jobs
                .current
                .as_ref()
                .is_some_and(|j| matches!(j.follow_up, FollowUp::Game | FollowUp::Arm))
            || self
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.follow, FollowUp::Game | FollowUp::Arm))
    }
    /// Desktop "结束演奏": release keys and ask the helper to exit.
    pub fn stop_game(&mut self) -> Result<()> {
        self.stop_game_with_close(true)
    }
    /// Phone stop keeps the helper armed so the next play command can reuse it.
    pub fn stop_game_playback(&mut self) -> Result<()> {
        self.stop_game_with_close(false)
    }
    pub(super) fn stop_game_with_close(&mut self, close: bool) -> Result<()> {
        self.jobs.clear_follow_up(FollowUp::Game);
        self.jobs.clear_follow_up(FollowUp::Arm);
        if let Some(p) = &mut self.pending {
            if matches!(p.follow, FollowUp::Game | FollowUp::Arm) {
                p.follow = FollowUp::None;
            }
        }
        if close {
            self.player.stop()?;
            self.status("已请求停止；演奏器会松开按键后退出。");
        } else {
            self.player.stop_playback()?;
            self.status("游戏演奏已停止。");
        }
        Ok(())
    }
    pub(super) fn update_playback(&mut self) -> Result<()> {
        if self.state.transport != Transport::Playing {
            return Ok(());
        }
        if self.last_audio_poll.elapsed() >= Duration::from_millis(100) {
            let position = self.audio.position()?;
            let playing = self.audio.playing()?;
            self.playback_clock.synchronize(position, playing, false);
            self.last_audio_poll = Instant::now();
            if !playing {
                self.state.transport = Transport::Ended;
                self.state.position_label = "试听结束".into();
                self.manual_seek = false;
            }
        }
        self.state.position = self
            .playback_clock
            .position(Some(self.state.preview_duration));
        self.state.logical_seek = self.to_score(self.state.position);
        self.events.push(ControllerEvent::Position);
        Ok(())
    }
    pub fn score_position_now(&mut self) -> f64 {
        if self.state.transport == Transport::Playing {
            let position = self
                .playback_clock
                .position(Some(self.state.preview_duration));
            self.to_score(position)
        } else {
            self.state.logical_seek
        }
    }
}

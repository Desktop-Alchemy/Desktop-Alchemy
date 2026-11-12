//! Local automation: pure rules, one arbiter and shared, demand-driven collectors.
pub mod types;
#[cfg(windows)]
mod windows;
use crate::core::{ActivationSource, Core};
use crate::model::UiMessage;
use anyhow::{bail, Result};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, watch};

#[derive(Default)]
struct Metrics {
    evaluations: std::sync::atomic::AtomicU64,
    switches: std::sync::atomic::AtomicU64,
    collector_wakes: std::sync::atomic::AtomicU64,
    process_scans: std::sync::atomic::AtomicU64,
}
pub use types::*;

#[derive(Debug)]
pub struct ActivationCancelled;
impl std::fmt::Display for ActivationCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("自动切页已取消")
    }
}
impl std::error::Error for ActivationCancelled {}

fn next_clock_wait(settings: &SceneSettings, now: chrono::DateTime<Local>) -> Duration {
    fn boundaries<'a>(c: &'a SceneCondition, out: &mut Vec<(&'a DayFilter, &'a str, bool)>) {
        match c {
            SceneCondition::All { children } | SceneCondition::Any { children } => {
                for c in children {
                    boundaries(c, out);
                }
            }
            SceneCondition::Not { child } => boundaries(child, out),
            SceneCondition::TimeRange { days, start, end } => {
                out.push((days, start, false));
                out.push((days, end, start > end));
            }
            _ => {}
        }
    }
    let mut times = vec![];
    for scene in settings.scenes.iter().filter(|s| s.enabled) {
        boundaries(&scene.condition, &mut times);
        if let SceneTrigger::Scheduled { schedule } = &scene.trigger {
            times.push((&schedule.days, &schedule.time, false));
        }
    }
    let mut wait = Duration::from_secs(24 * 3600);
    for (days, time, next_day) in times {
        let Some(minute) = minute(time) else {
            continue;
        };
        let dates: Vec<_> = if let DayFilter::Date { date } = days {
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .ok()
                .into_iter()
                .collect()
        } else {
            (-1..=8)
                .filter_map(|d| {
                    now.date_naive()
                        .checked_add_signed(chrono::Duration::days(d))
                })
                .collect()
        };
        for date in dates.into_iter().filter(|d| day_matches(days, *d)) {
            let date = if next_day {
                date.succ_opt().unwrap_or(date)
            } else {
                date
            };
            let Some(at) = date.and_hms_opt(minute / 60, minute % 60, 0) else {
                continue;
            };
            let local = Local.from_local_datetime(&at);
            for at in [local.earliest(), local.latest()].into_iter().flatten() {
                if let Ok(delta) = (at - now).to_std() {
                    if !delta.is_zero() {
                        wait = wait.min(delta);
                    }
                }
            }
        }
    }
    wait.max(Duration::from_millis(1))
}

fn minute(s: &str) -> Option<u32> {
    if s.len() != 5 {
        return None;
    }
    NaiveTime::parse_from_str(s, "%H:%M")
        .ok()
        .map(|t| t.hour() * 60 + t.minute())
}
fn validate_days(days: &DayFilter) -> Result<()> {
    match days {
        DayFilter::EveryDay => {}
        DayFilter::Weekdays { days }
            if !days.is_empty()
                && days.len() <= 7
                && days.iter().all(|d| (1..=7).contains(d))
                && days.iter().collect::<HashSet<_>>().len() == days.len() => {}
        DayFilter::Date { date }
            if NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok() && date.len() == 10 => {}
        _ => bail!(alchemy_device_api::UiFailure::new("backend.b297", "日期或星期无效")),
    }
    Ok(())
}
pub fn validate_condition(c: &SceneCondition, depth: usize, count: &mut usize) -> Result<()> {
    *count += 1;
    if depth > 8 || *count > 128 {
        bail!(alchemy_device_api::UiFailure::new("backend.b298", "条件最多 8 层、128 个节点"));
    }
    match c {
        SceneCondition::All { children } | SceneCondition::Any { children } => {
            if matches!(c, SceneCondition::Any { .. }) && children.is_empty() {
                bail!(alchemy_device_api::UiFailure::new("backend.b299", "任一满足组不能为空"));
            }
            for child in children {
                validate_condition(child, depth + 1, count)?;
            }
        }
        SceneCondition::Not { child } => validate_condition(child, depth + 1, count)?,
        SceneCondition::Process { matcher } | SceneCondition::Foreground { matcher } => {
            let (ProcessMatcher::Name { value } | ProcessMatcher::Path { value }) = matcher;
            if value.trim() != value
                || value.is_empty()
                || value.len() > 32768
                || value.chars().any(char::is_control)
            {
                bail!(alchemy_device_api::UiFailure::new("backend.b300", "程序匹配值无效"));
            }
            match matcher {
                ProcessMatcher::Name { value }
                    if value.contains(['/', '\\', '*', '?'])
                        || !value.to_lowercase().ends_with(".exe") =>
                {
                    bail!(alchemy_device_api::UiFailure::new("backend.b301", "请输入完整的 .exe 进程名"))
                }
                ProcessMatcher::Path { value }
                    if !std::path::Path::new(value).is_absolute()
                        || !value.to_lowercase().ends_with(".exe") =>
                {
                    bail!(alchemy_device_api::UiFailure::new("backend.b302", "请选择 .exe 的绝对路径"))
                }
                _ => {}
            }
        }
        SceneCondition::TimeRange { days, start, end } => {
            validate_days(days)?;
            if minute(start).is_none() || minute(end).is_none() || start == end {
                bail!(alchemy_device_api::UiFailure::new("backend.b303", "时间范围无效，起止时间不能相同"));
            }
        }
        SceneCondition::MediaPlaying => {}
    }
    Ok(())
}
pub fn validate(settings: &SceneSettings, pages: &[crate::model::Page]) -> Result<()> {
    if !(500..=10000).contains(&settings.process_interval_ms) || settings.scenes.len() > 200 {
        bail!(alchemy_device_api::UiFailure::new("backend.b304", "进程扫描间隔须为 500–10000 ms，场景最多 200 个"));
    }
    let exists = |id: &str| pages.iter().any(|p| p.id == id);
    if let SceneFallback::Page { page_id } = &settings.fallback {
        if !exists(page_id) {
            bail!(alchemy_device_api::UiFailure::new("backend.b305", "默认页面不存在"));
        }
    }
    let mut ids = HashSet::new();
    for s in &settings.scenes {
        if s.id.is_empty()
            || s.id.len() > 128
            || !ids.insert(&s.id)
            || s.name.trim().is_empty()
            || s.name.chars().count() > 120
            || s.name.chars().any(char::is_control)
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b306", "场景名称或 ID 无效"));
        }
        if s.enabled && !exists(&s.page_id) {
            bail!(alchemy_device_api::UiFailure::new("backend.b307", "场景目标页面不存在"));
        }
        if s.enter_delay_ms > 60000 || s.exit_delay_ms > 60000 {
            bail!(alchemy_device_api::UiFailure::new("backend.b308", "场景延迟须为 0–60 秒"));
        }
        if let SceneTrigger::Scheduled { schedule } = &s.trigger {
            validate_days(&schedule.days)?;
            if minute(&schedule.time).is_none() {
                bail!(alchemy_device_api::UiFailure::new("backend.b309", "触发时间无效"));
            }
        }
        validate_condition(&s.condition, 1, &mut 0)?;
    }
    Ok(())
}
fn day_matches(days: &DayFilter, date: NaiveDate) -> bool {
    match days {
        DayFilter::EveryDay => true,
        DayFilter::Weekdays { days } => days.contains(&(date.weekday().number_from_monday() as u8)),
        DayFilter::Date { date: s } => NaiveDate::parse_from_str(s, "%Y-%m-%d").ok() == Some(date),
    }
}
#[derive(Clone, Default)]
pub struct Facts {
    pub processes: Option<Vec<RunningProgram>>,
    pub foreground: Option<Option<RunningProgram>>,
    pub media: Option<bool>,
}
fn process_matches(m: &ProcessMatcher, p: &RunningProgram) -> ConditionTruth {
    use ConditionTruth::*;
    let normalize = |s: &str| s.replace('/', "\\").to_lowercase();
    match m {
        ProcessMatcher::Name { value } => {
            if p.name.eq_ignore_ascii_case(value) {
                True
            } else {
                False
            }
        }
        ProcessMatcher::Path { value } => match &p.path {
            Some(path) => {
                if normalize(value) == normalize(path) {
                    True
                } else {
                    False
                }
            }
            None => {
                if std::path::Path::new(value)
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&p.name))
                {
                    Unknown
                } else {
                    False
                }
            }
        },
    }
}
pub fn evaluate(c: &SceneCondition, f: &Facts, now: NaiveDateTime) -> ConditionTruth {
    use ConditionTruth::*;
    let aggregate = |values: Vec<ConditionTruth>, all: bool| {
        if values.contains(&if all { False } else { True }) {
            if all {
                False
            } else {
                True
            }
        } else if values.contains(&Unknown) {
            Unknown
        } else if all {
            True
        } else {
            False
        }
    };
    match c {
        SceneCondition::All { children } => {
            aggregate(children.iter().map(|c| evaluate(c, f, now)).collect(), true)
        }
        SceneCondition::Any { children } => aggregate(
            children.iter().map(|c| evaluate(c, f, now)).collect(),
            false,
        ),
        SceneCondition::Not { child } => match evaluate(child, f, now) {
            True => False,
            False => True,
            Unknown => Unknown,
        },
        SceneCondition::Process { matcher } => f.processes.as_ref().map_or(Unknown, |ps| {
            aggregate(
                ps.iter().map(|p| process_matches(matcher, p)).collect(),
                false,
            )
        }),
        SceneCondition::Foreground { matcher } => match &f.foreground {
            None => Unknown,
            Some(None) => False,
            Some(Some(p)) => process_matches(matcher, p),
        },
        SceneCondition::MediaPlaying => match f.media {
            Some(true) => True,
            Some(false) => False,
            None => Unknown,
        },
        SceneCondition::TimeRange { days, start, end } => {
            let (Some(start), Some(end)) = (minute(start), minute(end)) else {
                return Unknown;
            };
            let t = now.hour() * 60 + now.minute();
            let date = if start > end && t < end {
                now.date().pred_opt().unwrap_or(now.date())
            } else {
                now.date()
            };
            if day_matches(days, date)
                && if start < end {
                    t >= start && t < end
                } else {
                    t >= start || t < end
                }
            {
                True
            } else {
                False
            }
        }
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    pub processes: bool,
    pub foreground: bool,
    pub media: bool,
    pub clock: bool,
    pub paths: bool,
}
impl Needs {
    fn visit(&mut self, c: &SceneCondition) {
        match c {
            SceneCondition::All { children } | SceneCondition::Any { children } => {
                for c in children {
                    self.visit(c);
                }
            }
            SceneCondition::Not { child } => self.visit(child),
            SceneCondition::Process { matcher } => {
                self.processes = true;
                self.paths |= matches!(matcher, ProcessMatcher::Path { .. });
            }
            SceneCondition::Foreground { .. } => self.foreground = true,
            SceneCondition::MediaPlaying => self.media = true,
            SceneCondition::TimeRange { .. } => self.clock = true,
        }
    }
    fn for_settings(s: &SceneSettings) -> Self {
        let mut n = Self::default();
        if s.enabled && !s.paused {
            for s in s.scenes.iter().filter(|s| s.enabled) {
                n.visit(&s.condition);
                n.clock |= matches!(s.trigger, SceneTrigger::Scheduled { .. });
            }
        }
        n
    }
}
#[derive(Default)]
struct Stability {
    raw: bool,
    since: u64,
    active: bool,
}
#[derive(Default)]
pub struct Arbiter {
    stability: HashMap<String, Stability>,
    retained: Option<String>,
    fired: HashMap<String, NaiveDate>,
    fallback_target: Option<String>,
    previous: Option<NaiveDateTime>,
    matched: bool,
    base: String,
}
impl Arbiter {
    fn mark_fired(&mut self, id: &str, day: NaiveDate) -> bool {
        if self.fired.get(id).is_some_and(|last| *last >= day) {
            return false;
        }
        self.fired.insert(id.into(), day);
        true
    }
    fn reconfigure(&mut self, old: &SceneSettings, new: &SceneSettings) {
        self.stability.retain(|id, _| {
            old.scenes.iter().find(|r| &r.id == id) == new.scenes.iter().find(|r| &r.id == id)
        });
        if self.retained.as_ref().is_some_and(|id| {
            old.scenes.iter().find(|r| &r.id == id) != new.scenes.iter().find(|r| &r.id == id)
        }) {
            self.retained = None;
        }
        self.fired
            .retain(|id, _| new.scenes.iter().any(|r| &r.id == id));
    }
    pub fn reset(&mut self, base: String, now: NaiveDateTime) {
        self.stability.clear();
        self.retained = None;
        self.previous = Some(now);
        self.matched = false;
        self.fallback_target = None;
        self.base = base;
    }
    pub fn tick(
        &mut self,
        s: &SceneSettings,
        f: &Facts,
        now: NaiveDateTime,
        ms: u64,
    ) -> (Option<String>, SceneStatus, Option<u64>) {
        let previous = self.previous.replace(now);
        // Only evaluate schedules at their boundary; resume/time-change events reset this cursor.
        let crossed = previous.filter(|p| now >= *p);
        let mut new_retained = None;
        let mut status = SceneStatus::default();
        let mut active = vec![];
        let mut deadline = None;
        for scene in &s.scenes {
            let truth = evaluate(&scene.condition, f, now);
            let mut ready = false;
            if scene.enabled {
                match &scene.trigger {
                    SceneTrigger::Scheduled { schedule } => {
                        let due = minute(&schedule.time)
                            .and_then(|m| now.date().and_hms_opt(m / 60, m % 60, 0));
                        if due.is_some_and(|d| {
                            crossed.is_some_and(|p| {
                                p < d && now >= d && (now - d).num_milliseconds() <= 1500
                            })
                        }) && day_matches(&schedule.days, now.date())
                            && self.mark_fired(&scene.id, now.date())
                            && truth == ConditionTruth::True
                            && new_retained.is_none()
                        {
                            new_retained = Some(scene.id.clone());
                        }
                    }
                    SceneTrigger::Continuous => {
                        let st = self.stability.entry(scene.id.clone()).or_default();
                        let raw = truth == ConditionTruth::True;
                        if raw != st.raw {
                            st.raw = raw;
                            st.since = ms;
                        }
                        if truth == ConditionTruth::Unknown {
                            st.active = false;
                        }
                        let delay = if raw {
                            scene.enter_delay_ms
                        } else {
                            scene.exit_delay_ms
                        } as u64;
                        if raw != st.active {
                            let due = st.since.saturating_add(delay);
                            if ms >= due {
                                st.active = raw;
                            } else {
                                deadline = Some(deadline.map_or(due, |v: u64| v.min(due)));
                            }
                        }
                        ready = st.active;
                    }
                }
            }
            active.push(ready);
            status.states.push(SceneState {
                id: scene.id.clone(),
                truth,
                status: if scene.enabled {
                    "unmatched"
                } else {
                    "disabled"
                }
                .into(),
            });
        }
        if let Some(id) = new_retained {
            self.retained = Some(id);
        }
        if self
            .retained
            .as_ref()
            .is_some_and(|id| !s.scenes.iter().any(|r| r.id == *id && r.enabled))
        {
            self.retained = None;
        }
        for (i, scene) in s.scenes.iter().enumerate() {
            if self.retained.as_deref() == Some(&scene.id) {
                active[i] = true;
            }
        }
        let winner = active.iter().position(|v| *v);
        for (i, row) in status.states.iter_mut().enumerate() {
            if active[i] {
                row.status = if Some(i) == winner {
                    "active"
                } else {
                    "covered"
                }
                .into();
            } else if s.scenes[i].enabled && row.truth == ConditionTruth::Unknown {
                row.status = "unavailable".into();
            }
        }
        status.winner_id = winner.map(|i| s.scenes[i].id.clone());
        status.retained_id = self.retained.clone();
        let target = if let Some(i) = winner {
            self.matched = true;
            self.fallback_target = None;
            Some(s.scenes[i].page_id.clone())
        } else {
            if std::mem::take(&mut self.matched) {
                self.fallback_target = match &s.fallback {
                    SceneFallback::Keep => None,
                    SceneFallback::Return => Some(self.base.clone()),
                    SceneFallback::Page { page_id } => Some(page_id.clone()),
                };
            }
            self.fallback_target.clone()
        };
        (target, status, deadline)
    }
}

pub struct SceneManager {
    shared: std::sync::OnceLock<Arc<SharedCollectors>>,
    metrics: Arc<Metrics>,
    core: Arc<Core>,
    status: Mutex<SceneStatus>,
    pub changed: broadcast::Sender<SceneStatus>,
    retry: tokio::sync::Notify,
    shutdown: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

#[cfg(test)]
mod tests;
impl SceneManager {
    pub fn new(core: Arc<Core>) -> Arc<Self> {
        let (changed, _) = broadcast::channel(16);
        let (shutdown, _) = watch::channel(false);
        Arc::new(Self {
            shared: std::sync::OnceLock::new(),
            metrics: Arc::new(Metrics::default()),
            core,
            status: Mutex::new(SceneStatus::default()),
            changed,
            retry: tokio::sync::Notify::new(),
            shutdown,
            task: Mutex::new(None),
        })
    }
    pub fn start(self: &Arc<Self>) {
        let m = self.clone();
        *self.task.lock().unwrap() = Some(tauri::async_runtime::spawn(async move {
            m.run().await;
        }));
    }
    pub fn use_collectors(&self, collectors: Arc<SharedCollectors>) {
        let _ = self.shared.set(collectors);
    }
    pub fn status(&self) -> SceneStatus {
        self.status.lock().unwrap().clone()
    }
    pub fn retry(&self) {
        self.retry.notify_one();
    }
    fn publish(&self, mut value: SceneStatus) {
        value.ui_message = value.error.as_ref().map(|_| UiMessage::new("errors.sceneFailed"));
        let mut status = self.status.lock().unwrap();
        if *status != value {
            *status = value.clone();
            let _ = self.changed.send(value);
        }
    }
    pub async fn shutdown(&self) {
        self.core.stop_scenes();
        let _ = self.shutdown.send(true);
        let task = self.task.lock().unwrap().take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
    pub async fn programs(&self) -> Result<Vec<RunningProgram>> {
        #[cfg(windows)]
        {
            tokio::task::spawn_blocking(|| windows::processes(true)).await?
        }
        #[cfg(not(windows))]
        {
            bail!(alchemy_device_api::UiFailure::new("errors.processPlatform", "仅 Windows 支持程序检测"))
        }
    }
    pub async fn check(&self, condition: SceneCondition) -> Result<ConditionTruth> {
        validate_condition(&condition, 1, &mut 0)?;
        let mut needs = Needs::default();
        needs.visit(&condition);
        #[cfg(windows)]
        {
            let facts = tokio::task::spawn_blocking(move || windows::sample(needs)).await?;
            Ok(evaluate(&condition, &facts, Local::now().naive_local()))
        }
        #[cfg(not(windows))]
        {
            Ok(evaluate(
                &condition,
                &Facts::default(),
                Local::now().naive_local(),
            ))
        }
    }
    async fn run(self: Arc<Self>) {
        let mut changes = self.core.changed.subscribe();
        let mut stop = self.shutdown.subscribe();
        let started = Instant::now();
        let mut arbiter = Arbiter::default();
        let mut saved: Option<SceneSettings> = None;
        let mut pending: Option<String> = None;
        let mut failed: Option<String> = None;
        let mut next_switch = Instant::now();
        let mut restart_collectors = false;
        #[cfg(windows)]
        let mut resync = 0;
        #[cfg(windows)]
        let mut collector: Option<windows::Collector> = None;
        let (signal, mut updates) = tokio::sync::mpsc::channel::<()>(1);
        let (fallback, _) = broadcast::channel(1);
        let mut shared_updates = self
            .shared
            .get()
            .map(|s| s.changed.subscribe())
            .unwrap_or_else(|| fallback.subscribe());
        loop {
            let next_tick;
            let generation = self.core.scene_generation();
            let snap = self.core.snapshot();
            let settings = &snap.config.scene_settings;
            if saved.as_ref() != Some(settings) || restart_collectors {
                restart_collectors = false;
                let old_running = saved.as_ref().is_some_and(|s| s.enabled && !s.paused);
                let running = settings.enabled && !settings.paused;
                if !old_running || !running {
                    arbiter.reset(snap.active_page.clone(), Local::now().naive_local());
                } else if let Some(old) = &saved {
                    arbiter.reconfigure(old, settings);
                }
                saved = Some(settings.clone());
                pending = None;
                failed = None;
                #[cfg(windows)]
                {
                    if let Some(c) = collector.take() {
                        c.stop().await;
                    }
                    resync = 0;
                    let needs = Needs::for_settings(settings);
                    if self.shared.get().is_none() && running && needs != Needs::default() {
                        collector = Some(windows::Collector::start(
                            needs,
                            settings,
                            signal.clone(),
                            self.core.clone(),
                            self.metrics.clone(),
                        ));
                    }
                }
            }
            if *stop.borrow() || *self.core.stop.borrow() {
                break;
            }
            let running = settings.enabled && !settings.paused;
            if running {
                self.metrics
                    .evaluations
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let source_epoch = self.core.scene_source_epoch();
                #[cfg(windows)]
                let facts = self
                    .shared
                    .get()
                    .map(|s| s.facts.lock().unwrap().clone())
                    .unwrap_or_else(|| collector.as_ref().map(|c| c.facts()).unwrap_or_default());
                #[cfg(not(windows))]
                let facts = Facts::default();
                let now = Local::now().naive_local();
                #[cfg(windows)]
                {
                    let current = self
                        .shared
                        .get()
                        .map(|s| s.resync.load(std::sync::atomic::Ordering::SeqCst))
                        .unwrap_or_else(|| collector.as_ref().map(|c| c.resync()).unwrap_or(0));
                    if resync != current {
                        resync = current;
                        arbiter.previous = Some(now);
                        arbiter.stability.clear();
                    }
                }
                let ms = started.elapsed().as_millis() as u64;
                let (target, mut status, deadline) = arbiter.tick(settings, &facts, now, ms);
                let (repaint, frame_error) = self.core.scene_frame_state();
                if let Some(error) = frame_error {
                    status.error = Some(error);
                    failed = Some(snap.active_page.clone());
                }
                if let Some(target) = target {
                    pending = Some(target);
                }
                // No match invalidates a previous candidate even when fallback is Keep.
                if status.winner_id.is_none()
                    && !arbiter.matched
                    && matches!(settings.fallback, SceneFallback::Keep)
                {
                    pending = None;
                }
                if !repaint && pending.as_ref().is_some_and(|id| *id == snap.active_page) {
                    pending = None;
                }
                if Instant::now() >= next_switch {
                    if let Some(id) = pending.take().filter(|id| failed.as_ref() != Some(id)) {
                        let id = if snap.config.pages.iter().any(|p| p.id == id) {
                            id
                        } else {
                            snap.config.pages[0].id.clone()
                        };
                        match self
                            .core
                            .activate_from(
                                &id,
                                ActivationSource::Scene {
                                    generation,
                                    source_epoch,
                                    valid_until: Instant::now()
                                        + next_clock_wait(settings, Local::now()),
                                },
                            )
                            .await
                        {
                            Ok(()) => {
                                self.metrics
                                    .switches
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                failed = None;
                            }
                            Err(e) if e.is::<ActivationCancelled>() => {
                                pending = Some(id);
                            }
                            Err(e) => {
                                status.error = Some(e.to_string());
                                failed = Some(id);
                            }
                        }
                        next_switch = Instant::now() + Duration::from_millis(500);
                    }
                }
                if status.error.is_none() && failed.is_some() {
                    status.error = self.status().error;
                }
                self.publish(status);
                let mut wait = next_clock_wait(settings, Local::now()).as_millis() as u64;
                if let Some(d) = deadline {
                    wait = wait.min(d.saturating_sub(ms).max(1));
                }
                if pending.is_some() {
                    wait = wait.min(
                        next_switch
                            .saturating_duration_since(Instant::now())
                            .as_millis()
                            .max(1) as u64,
                    );
                }
                next_tick = Instant::now() + Duration::from_millis(wait);
            } else {
                self.publish(SceneStatus::default());
                next_tick = Instant::now() + Duration::from_secs(3600);
            }
            tokio::select! {
                _ = stop.changed() => break,
                _ = self.retry.notified() => { failed = None; self.core.retry_scene_frame(); restart_collectors = true; },
                _ = updates.recv() => {},
                _ = shared_updates.recv() => {},
                _ = changes.recv() => {},
                _ = tokio::time::sleep_until(next_tick.into()) => {},
            }
        }
        #[cfg(windows)]
        if let Some(c) = collector.take() {
            c.stop().await;
        }
    }
}

/// One demand-driven Windows collector for every device arbiter.
pub struct SharedCollectors {
    facts: Mutex<Facts>,
    resync: std::sync::atomic::AtomicU64,
    changed: broadcast::Sender<()>,
    stop: tokio_util::sync::CancellationToken,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}
impl SharedCollectors {
    pub fn new() -> Arc<Self> {
        let (changed, _) = broadcast::channel(16);
        Arc::new(Self {
            facts: Mutex::new(Facts::default()),
            resync: Default::default(),
            changed,
            stop: tokio_util::sync::CancellationToken::new(),
            task: Mutex::new(None),
        })
    }
    pub fn start(self: &Arc<Self>, manager: Arc<crate::devices::DeviceManager>) {
        let this = self.clone();
        *self.task.lock().unwrap() = Some(tauri::async_runtime::spawn(async move {
            let (signal, mut updates) = tokio::sync::mpsc::channel(1);
            let mut changes = manager.changed.subscribe();
            let mut stop = manager.host.stop.subscribe();
            let mut saved = None;
            #[cfg(windows)]
            let mut collector: Option<windows::Collector> = None;
            loop {
                let mut settings = SceneSettings {
                    enabled: true,
                    process_interval_ms: 10000,
                    ..Default::default()
                };
                for runtime in manager.runtimes() {
                    let own = runtime.core.snapshot().config.scene_settings;
                    if own.enabled && !own.paused {
                        settings.process_interval_ms =
                            settings.process_interval_ms.min(own.process_interval_ms);
                        settings.scenes.extend(own.scenes);
                    }
                }
                if saved.as_ref() != Some(&settings) {
                    #[cfg(windows)]
                    {
                        if let Some(old) = collector.take() {
                            old.stop().await;
                        }
                        let needs = Needs::for_settings(&settings);
                        if needs != Needs::default() {
                            collector = Some(windows::Collector::start(
                                needs,
                                &settings,
                                signal.clone(),
                                manager.host.clone(),
                                Arc::new(Metrics::default()),
                            ));
                        }
                    }
                    saved = Some(settings);
                }
                #[cfg(windows)]
                {
                    *this.facts.lock().unwrap() =
                        collector.as_ref().map(|c| c.facts()).unwrap_or_default();
                    this.resync.store(
                        collector.as_ref().map(|c| c.resync()).unwrap_or(0),
                        std::sync::atomic::Ordering::SeqCst,
                    );
                }
                let _ = this.changed.send(());
                tokio::select! {
                    _ = this.stop.cancelled() => break,
                    _ = stop.changed() => break,
                    _ = changes.recv() => {},
                    _ = updates.recv() => {},
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {},
                }
            }
            #[cfg(windows)]
            if let Some(old) = collector.take() {
                old.stop().await;
            }
        }));
    }
    pub async fn shutdown(&self) {
        self.stop.cancel();
        let task = self.task.lock().unwrap().take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

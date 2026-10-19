use crate::engine::Engine;
use crate::game::Game;
use crate::site::{Account, Report, Site, now_ms};
use crate::zip;
use serde_json::{Value, json};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

const CHUNK: usize = 4 << 20;
const IDLE: Duration = Duration::from_secs(120);
const MAX_AGE: Duration = Duration::from_secs(6 * 3600);
const TICK: Duration = Duration::from_secs(1);

pub enum Event {
    Step(String),
    Progress(f32),
    Report(String),
}

#[derive(Clone, Default)]
pub struct Target {
    pub region: Value,
    pub guild: Option<Value>,
    pub visibility: Option<Value>,
    pub description: String,
}

pub struct Session {
    pub game: &'static Game,
    pub site: Site,
    pub account: Account,
    pub engine: Engine,
}

struct Part {
    lines: Vec<String>,
    span: (u64, u64),
    eof: bool,
}

struct Run {
    code: String,
    segment: i64,
    live: bool,
}

struct Reader {
    file: File,
    pos: u64,
    latin1: bool,
}

impl Reader {
    fn open(path: &Path, latin1: bool) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self { file, pos: 0, latin1 })
    }

    fn len(&self) -> u64 {
        self.file.metadata().map_or(0, |m| m.len())
    }

    fn next(&mut self, flush: bool) -> Result<Part, String> {
        let start = self.pos;
        let mut buf = Vec::with_capacity(CHUNK);
        self.file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
        (&mut self.file)
            .take(CHUNK as u64)
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;

        let cut = match buf.iter().rposition(|&b| b == b'\n') {
            Some(i) => i + 1,
            None if flush || buf.len() == CHUNK => buf.len(),
            None => 0,
        };
        buf.truncate(cut);
        self.pos = start + cut as u64;

        let lines = buf
            .split(|&b| b == b'\n')
            .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
            .filter(|l| !l.is_empty())
            .map(|l| match self.latin1 {
                true => l.iter().map(|&b| b as char).collect(),
                false => String::from_utf8_lossy(l).into_owned(),
            })
            .collect();

        Ok(Part {
            lines,
            span: (start, self.pos),
            eof: self.pos >= self.len(),
        })
    }
}

impl Session {
    pub fn open(
        game: &'static Game,
        email: &str,
        password: &str,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Self, String> {
        let mut site = Site::new(game);
        notify(Event::Step(format!("logging in to {}", game.host)));
        let account = site.login(email, password)?;

        notify(Event::Step("loading the site's parser".into()));
        let engine = Engine::load(site.parser()?)?;
        Ok(Self {
            game,
            site,
            account,
            engine,
        })
    }

    fn begin(&mut self, target: &Target, file: &str, live: bool) -> Result<Run, String> {
        self.engine.reset()?;

        let region = match &target.guild {
            Some(g) => self
                .account
                .guilds
                .iter()
                .find(|c| c.value == *g)
                .and_then(|c| c.region.as_ref()),
            None => None,
        };
        let report = Report {
            file,
            guild: target.guild.as_ref(),
            region: Some(region.unwrap_or(&target.region)),
            visibility: target.visibility.as_ref(),
            description: &target.description,
            parser: self.engine.version,
            mode: if live { 1 } else { 2 },
        };

        let code = self.site.create(&report)?;
        self.engine.report_code(&code)?;
        Ok(Run { code, segment: 1, live })
    }

    fn part(&mut self, run: &mut Run, region: &Value, file: &str, part: Part, push: bool) -> Result<bool, String> {
        if part.lines.is_empty() && !push {
            return Ok(false);
        }

        if !part.lines.is_empty() {
            self.engine.parse(part.lines, region, file, part.span)?;
        }

        let fights = self.engine.fights(push)?;
        if fights.count == 0 {
            return Ok(false);
        }

        let (master, players) = self.engine.master(&run.code)?;
        self.site.master(&run.code, run.segment, &players, zip::pack(&master))?;
        drop(master);

        let events = zip::pack(&self.engine.fights_text()?);
        let parameters = json!({
            "startTime": fights.start,
            "endTime": fights.end,
            "mythic": fights.mythic,
            "isLiveLog": run.live,
            "isRealTime": false,
            "inProgressEventCount": 0,
            "segmentId": run.segment,
        });

        let next = self.site.segment(&run.code, parameters, events)?;
        if next > 0 {
            run.segment = next;
        }

        self.engine.clear()?;
        Ok(true)
    }

    fn finish<T>(&mut self, run: &Run, result: Result<T, String>) -> Result<T, String> {
        let reset = self.engine.reset();
        let end = self.site.terminate(&run.code);
        let value = result?;
        reset.and(end).map(|()| value)
    }

    pub fn upload(
        &mut self,
        path: &Path,
        target: &Target,
        cancel: &AtomicBool,
        notify: &mut dyn FnMut(Event),
    ) -> Result<String, String> {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let mut reader = Reader::open(path, self.game.latin1)?;

        notify(Event::Step(format!("uploading {name}")));
        let mut run = self.begin(target, &name, false)?;
        notify(Event::Report(self.site.report_url(&run.code)));

        let result = self.feed(&mut run, &mut reader, path, target, cancel, notify);
        match self.finish(&run, result)? {
            0 => Err(format!("{name}: no fights found")),
            _ => Ok(self.site.report_url(&run.code)),
        }
    }

    fn feed(
        &mut self,
        run: &mut Run,
        reader: &mut Reader,
        path: &Path,
        target: &Target,
        cancel: &AtomicBool,
        notify: &mut dyn FnMut(Event),
    ) -> Result<usize, String> {
        if let Some(date) = self.game.start_date(path) {
            self.engine.start_date(&date)?;
        }

        let file = path.to_string_lossy().into_owned();
        let size = reader.len().max(1);
        let mut fights = 0;

        while !cancel.load(Ordering::Relaxed) {
            let part = reader.next(true)?;
            let eof = part.eof;

            fights += self.part(run, &target.region, &file, part, eof)? as usize;
            notify(Event::Progress(reader.pos as f32 / size as f32));

            if eof {
                break;
            }
        }

        Ok(fights)
    }

    pub fn live(
        &mut self,
        dir: &Path,
        target: &Target,
        all: bool,
        cancel: &AtomicBool,
        notify: &mut dyn FnMut(Event),
    ) -> Result<String, String> {
        let mut run = self.begin(target, "live.log", true)?;
        let url = self.site.report_url(&run.code);
        notify(Event::Report(url.clone()));

        let result = self.follow(&mut run, dir, target, all, cancel, notify);
        self.finish(&run, result).map(|()| url)
    }

    fn follow(
        &mut self,
        run: &mut Run,
        dir: &Path,
        target: &Target,
        all: bool,
        cancel: &AtomicBool,
        notify: &mut dyn FnMut(Event),
    ) -> Result<(), String> {
        if !all {
            self.engine.live_start(now_ms())?;
        }

        let mut current: Option<(PathBuf, Reader)> = None;
        let mut pushed = true;
        let mut waiting = false;

        loop {
            let stop = cancel.load(Ordering::Relaxed);
            let newest = self
                .game
                .logs(dir)
                .into_iter()
                .next()
                .filter(|l| l.modified.elapsed().unwrap_or_default() < MAX_AGE);

            if let Some(log) = newest.filter(|l| current.as_ref().is_none_or(|(p, _)| *p != l.path)) {
                notify(Event::Step(format!("watching {}", log.path.display())));
                if let Some(date) = self.game.start_date(&log.path) {
                    self.engine.start_date(&date)?;
                }
                current = Some((log.path.clone(), Reader::open(&log.path, self.game.latin1)?));
            }

            let Some((path, reader)) = current.as_mut() else {
                if stop {
                    return Ok(());
                }
                if !waiting {
                    notify(Event::Step(format!(
                        "waiting for a {} log in {}",
                        self.game.name,
                        dir.display()
                    )));
                    waiting = true;
                }
                std::thread::sleep(TICK);
                continue;
            };

            let part = reader.next(stop)?;
            let fresh = !part.lines.is_empty();
            let idle = std::fs::metadata(&*path)
                .and_then(|m| m.modified())
                .map_or(true, |m| SystemTime::now().duration_since(m).unwrap_or_default() > IDLE);

            pushed &= !fresh;
            let push = stop || (idle && !pushed);
            let file = path.to_string_lossy().into_owned();

            if self.part(run, &target.region, &file, part, push)? {
                notify(Event::Step("uploaded new fights".into()));
            }
            pushed |= push;

            if stop {
                return Ok(());
            }
            if !fresh {
                std::thread::sleep(TICK);
            }
        }
    }
}

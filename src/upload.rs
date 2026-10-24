use crate::engine::{Engine, Scan};
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

#[derive(Clone, Debug, Default)]
pub struct Stamp {
    pub text: String,
    pub year: i64,
    pub shift: i64,
}

#[derive(Clone, Debug)]
pub struct Fight {
    pub name: String,
    pub start: i64,
    pub end: i64,
    pub boss: i64,
    pub difficulty: i64,
    pub success: bool,
    pub pulls: i64,
    pub stamps: Option<(Stamp, Stamp)>,
}

#[derive(Clone, Default)]
pub struct Filter {
    pub trash: bool,
    pub wipes: bool,
    pub failed: bool,
    pub short: bool,
    pub fights: Option<Vec<Fight>>,
}

impl Fight {
    pub fn key(&self, game: &Game) -> bool {
        game.id == "wow" && self.difficulty == 8 && self.boss > 0
    }

    pub fn dungeon(&self) -> bool {
        self.boss > 0 && matches!(self.difficulty, 8 | 10)
    }

    pub fn columns(&self, game: &Game) -> [String; 4] {
        let result = match (self.boss, self.key(game), self.success) {
            (0, ..) => "trash",
            (_, true, true) => "timed",
            (_, true, false) => "failed",
            (_, false, true) => "kill",
            _ => "wipe",
        };
        let pulls = match self.pulls {
            0 | 1 => String::new(),
            n => format!(" x{n}"),
        };
        let secs = (self.end - self.start).max(0) / 1000;
        [
            self.name.clone(),
            game.difficulty(self.difficulty).to_owned(),
            format!("{result}{pulls}"),
            format!("{}:{:02}", secs / 60, secs % 60),
        ]
    }

    pub fn label(&self, game: &Game) -> String {
        let [name, difficulty, result, time] = self.columns(game);
        let title = match difficulty.is_empty() {
            true => name,
            false => format!("{name} {difficulty}"),
        };
        format!("{title}  {result}  {time}")
    }
}

impl Filter {
    pub fn scans(&self) -> bool {
        self.trash || self.wipes || self.failed || self.short
    }

    pub fn keep(&self, game: &Game, f: &Fight) -> bool {
        let key = f.key(game);
        !(self.trash && f.boss == 0
            || self.wipes && f.boss > 0 && !key && !f.success
            || self.failed && key && !f.success
            || self.short && f.end - f.start < crate::split::SHORT)
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

pub struct Session {
    pub game: &'static Game,
    pub site: Site,
    pub account: Account,
    pub engine: Engine,
}

struct Part {
    text: String,
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
    key: Option<String>,
}

impl Reader {
    fn open(path: &Path, latin1: bool) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self {
            file,
            pos: 0,
            latin1,
            key: None,
        })
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

        let text = match self.latin1 {
            true => buf.iter().map(|&b| b as char).collect(),
            false => String::from_utf8(buf).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
        };

        Ok(Part {
            text: self.reloads(text),
            span: (start, self.pos),
            eof: self.pos >= self.len(),
        })
    }

    fn reloads(&mut self, text: String) -> String {
        if !text.contains("CHALLENGE_MODE_") {
            return text;
        }

        let mut kept = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            let event = line.split_once("  ").map_or("", |(_, e)| e);
            if let Some(rest) = event.strip_prefix("CHALLENGE_MODE_START,") {
                let map = rest.rsplit_once('"').map_or(rest, |(_, t)| t.trim_start_matches(','));
                let map = map.split(',').next().unwrap_or_default().to_owned();
                if self.key.as_ref() == Some(&map) {
                    continue;
                }
                self.key = Some(map);
            } else if event.starts_with("CHALLENGE_MODE_END,") {
                self.key = None;
            }
            kept.push_str(line);
        }
        kept
    }
}

fn parser_scan(
    engine: &Engine,
    game: &Game,
    path: &Path,
    region: &Value,
    cancel: &AtomicBool,
    notify: &mut dyn FnMut(Event),
) -> Result<Vec<Fight>, String> {
    engine.reset()?;
    if let Some(date) = game.start_date(path) {
        engine.start_date(&date)?;
    }

    let mut reader = Reader::open(path, game.latin1)?;
    let file = path.to_string_lossy().into_owned();
    let size = reader.len().max(1);
    let scan = Scan {
        raids: &[],
        scanning: true,
    };

    while !cancel.load(Ordering::Relaxed) {
        let part = reader.next(true)?;
        engine.parse(part.text, region, &file, part.span, scan)?;
        notify(Event::Progress(reader.pos as f32 / size as f32));
        if part.eof {
            break;
        }
    }

    let fights = engine.scanned();
    engine.reset()?;

    let list: Vec<Value> = serde_json::from_str(&fights?).map_err(|e| e.to_string())?;
    let num = |v: &Value, k: &str| v[k].as_f64().unwrap_or(0.0) as i64;
    Ok(list
        .iter()
        .map(|v| Fight {
            name: v["name"].as_str().unwrap_or("Unknown").to_owned(),
            start: num(v, "start"),
            end: num(v, "end"),
            boss: num(v, "boss"),
            difficulty: num(v, "difficulty"),
            success: v["success"].as_bool().unwrap_or(false),
            pulls: num(v, "pulls"),
            stamps: None,
        })
        .collect())
}

pub fn fights(
    game: &'static Game,
    path: &Path,
    cancel: &AtomicBool,
    notify: &mut dyn FnMut(Event),
) -> Result<Option<Vec<Fight>>, String> {
    if crate::scan::supported(game) {
        return crate::scan::scan(path, cancel, notify).map(Some);
    }
    let Some(scripts) = crate::store::load_parser(game.host) else {
        return Ok(None);
    };
    let engine = Engine::load(scripts)?;
    parser_scan(&engine, game, path, &Value::from(0), cancel, notify).map(Some)
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
        let scripts = site.parser()?;
        crate::store::save_parser(game.host, &scripts);
        let engine = Engine::load(scripts)?;
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

    fn part(
        &mut self,
        run: &mut Run,
        region: &Value,
        file: &str,
        part: Part,
        push: bool,
        raids: &[(i64, i64)],
    ) -> Result<bool, String> {
        if part.text.is_empty() && !push {
            return Ok(false);
        }

        if !part.text.is_empty() {
            let scan = Scan { raids, scanning: false };
            self.engine.parse(part.text, region, file, part.span, scan)?;
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
        files: &[PathBuf],
        target: &Target,
        filter: &Filter,
        cancel: &AtomicBool,
        notify: &mut dyn FnMut(Event),
    ) -> Result<String, String> {
        let mut files = files.to_vec();
        files.sort_by_key(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok());

        let mut plan = Vec::new();
        for path in files {
            let raids = match (&filter.fights, filter.scans()) {
                (Some(fights), _) => self.ranges(fights)?,
                (None, true) => {
                    notify(Event::Step(format!("scanning {}", file_name(&path))));
                    let fights = self.scan(&path, &target.region, cancel, notify)?;
                    let kept: Vec<_> = fights.into_iter().filter(|f| filter.keep(self.game, f)).collect();
                    let kept = self.ranges(&kept)?;
                    if kept.is_empty() {
                        notify(Event::Step(format!(
                            "{}: every fight was filtered out",
                            file_name(&path)
                        )));
                        continue;
                    }
                    kept
                }
                (None, false) => Vec::new(),
            };
            plan.push((path, raids));
        }

        let Some((first, _)) = plan.first() else {
            return Err("no fights left to upload".into());
        };
        let name = match plan.len() {
            1 => file_name(first),
            n => format!("{} +{}", file_name(first), n - 1),
        };

        notify(Event::Step(format!("uploading {name}")));
        let mut run = self.begin(target, &name, false)?;
        let url = self.site.report_url(&run.code);
        notify(Event::Report(url.clone()));

        let result = self.feed_all(&mut run, &plan, target, cancel, notify);
        match self.finish(&run, result)? {
            0 => Err(format!("{name}: no fights found")),
            _ => Ok(url),
        }
    }

    fn feed_all(
        &mut self,
        run: &mut Run,
        plan: &[(PathBuf, Vec<(i64, i64)>)],
        target: &Target,
        cancel: &AtomicBool,
        notify: &mut dyn FnMut(Event),
    ) -> Result<usize, String> {
        let total = plan
            .iter()
            .map(|(p, _)| std::fs::metadata(p).map_or(0, |m| m.len()))
            .sum::<u64>()
            .max(1);
        let (mut done, mut fights) = (0, 0);

        for (path, raids) in plan {
            if plan.len() > 1 {
                notify(Event::Step(format!("uploading {}", file_name(path))));
            }
            let mut reader = Reader::open(path, self.game.latin1)?;
            if let Some(date) = self.game.start_date(path) {
                self.engine.start_date(&date)?;
            }

            let file = path.to_string_lossy().into_owned();
            while !cancel.load(Ordering::Relaxed) {
                let part = reader.next(true)?;
                let eof = part.eof;

                fights += self.part(run, &target.region, &file, part, eof, raids)? as usize;
                notify(Event::Progress((done + reader.pos) as f32 / total as f32));

                if eof {
                    break;
                }
            }
            done += reader.len();
        }

        Ok(fights)
    }

    fn ranges(&self, fights: &[Fight]) -> Result<Vec<(i64, i64)>, String> {
        let stamps: Vec<_> = fights
            .iter()
            .filter_map(|f| f.stamps.as_ref())
            .flat_map(|(a, b)| [(a.text.as_str(), a.year), (b.text.as_str(), b.year)])
            .collect();
        let mut times = self.engine.stamps(&stamps)?.into_iter();
        let shifts = fights.iter().filter_map(|f| f.stamps.as_ref());
        let mut shifts = shifts.flat_map(|(a, b)| [a.shift, b.shift]);
        let mut at = || Ok::<_, String>(times.next().ok_or("missing timestamp")? + shifts.next().unwrap_or(0));

        fights
            .iter()
            .map(|f| match f.stamps {
                Some(_) => Ok((at()?, at()?)),
                None => Ok((f.start, f.end)),
            })
            .collect()
    }

    pub fn scan(
        &mut self,
        path: &Path,
        region: &Value,
        cancel: &AtomicBool,
        notify: &mut dyn FnMut(Event),
    ) -> Result<Vec<Fight>, String> {
        match crate::scan::supported(self.game) {
            true => crate::scan::scan(path, cancel, notify),
            false => parser_scan(&self.engine, self.game, path, region, cancel, notify),
        }
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
            let fresh = !part.text.is_empty();
            let idle = std::fs::metadata(&*path)
                .and_then(|m| m.modified())
                .map_or(true, |m| SystemTime::now().duration_since(m).unwrap_or_default() > IDLE);

            pushed &= !fresh;
            let push = stop || (idle && !pushed);
            let file = path.to_string_lossy().into_owned();

            if self.part(run, &target.region, &file, part, push, &[])? {
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

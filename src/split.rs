use crate::game::Game;
use crate::upload::Event;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const GAP: i64 = 4 * 3600 * 1000;
pub const SHORT: i64 = 10_000;

#[derive(Default)]
struct Line {
    time: i64,
    zone: i64,
    force: bool,
    header: bool,
    reset: bool,
    event: Option<(String, String, String)>,
}

#[derive(Default)]
struct State {
    time: i64,
    zone: i64,
    key: bool,
    last: Option<(String, String)>,
}

fn civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy
}

fn stamp(parts: &[i64]) -> i64 {
    match parts {
        [y, mo, d, h, mi, s, ..] => ((civil(*y, *mo, *d) * 24 + h) * 60 + mi) * 60_000 + s * 1000,
        _ => 0,
    }
}

fn numbers(text: &str) -> Vec<i64> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .take(7)
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

fn quoted(text: &str) -> Option<(&str, &str)> {
    let open = text.find('"')?;
    let close = text.rfind('"')?;
    (close > open).then(|| (&text[open + 1..close], &text[close + 1..]))
}

fn wow(game: &Game, text: &str) -> Line {
    let Some((when, rest)) = text.split_once("  ") else {
        return Line::default();
    };
    let n = numbers(when);
    let time = match when.split(' ').next().map(|d| d.matches('/').count()) {
        Some(2) => stamp(&[n[2], n[0], n[1], n[3], n[4], n[5]]),
        _ if n.len() >= 5 => stamp(&[2000, n[0], n[1], n[2], n[3], n[4]]),
        _ => 0,
    };

    let kind = rest.split(',').next().unwrap_or_default();
    let event = match kind {
        "ENCOUNTER_START" | "CHALLENGE_MODE_START" if game.id == "wow" => quoted(rest).map(|(label, tail)| {
            let fields: Vec<&str> = tail.split(',').collect();
            let difficulty = match kind {
                "ENCOUNTER_START" => game.difficulty(fields.get(1).and_then(|d| d.parse().ok()).unwrap_or(0)),
                _ => fields.get(3).copied().unwrap_or_default(),
            };
            (kind.to_owned(), label.to_owned(), difficulty.to_owned())
        }),
        "CHALLENGE_MODE_END" if game.id == "wow" => Some((kind.to_owned(), String::new(), String::new())),
        _ => None,
    };

    Line {
        time,
        header: text.contains("COMBAT_LOG_VERSION"),
        event,
        ..Line::default()
    }
}

fn line(game: &Game, text: &str) -> Line {
    let fields: Vec<&str> = text.splitn(5, |c| c == '|' || (game.id == "eso" && c == ',')).collect();
    let field = |i: usize| fields.get(i).copied().unwrap_or_default();

    match game.id {
        "ff" => Line {
            time: stamp(&numbers(field(1))),
            zone: match field(0) {
                "01" => i64::from_str_radix(field(2), 16).unwrap_or(0),
                _ => 0,
            },
            header: matches!(field(0), "250" | "253") || field(0) == "251" && text.contains("DeucalionClient"),
            reset: field(0) == "253",
            ..Line::default()
        },
        "eso" => Line {
            force: field(1) == "BEGIN_LOG",
            ..Line::default()
        },
        "fellowship" => Line {
            time: stamp(&numbers(field(0))),
            zone: match field(1) {
                "ZONE_CHANGE" => field(3).parse().unwrap_or(0),
                _ => 0,
            },
            header: field(1) == "LOGGING_STARTED",
            reset: field(1) == "LOGGING_STARTED",
            ..Line::default()
        },
        _ => wow(game, text),
    }
}

fn split_here(game: &Game, line: &Line, state: &mut State) -> bool {
    if let Some((kind, label, difficulty)) = &line.event {
        let key = || (label.clone(), difficulty.clone());
        match kind.as_str() {
            "CHALLENGE_MODE_END" => {
                state.key = false;
                return false;
            }
            "CHALLENGE_MODE_START" => {
                state.key = true;
                return true;
            }
            _ if state.key => {
                let left = difficulty != game.difficulty(8);
                if left {
                    state.key = false;
                    state.last = Some(key());
                }
                return left;
            }
            _ => {
                let changed = state.last.as_ref() != Some(&key());
                state.last = Some(key());
                return changed;
            }
        }
    }

    let gap = state.time > 0 && line.time > state.time + GAP;
    match game.id {
        "eso" => line.force,
        "ff" | "fellowship" => gap || state.zone > 0 && line.zone > 0 && state.zone != line.zone,
        _ => gap,
    }
}

fn piece(path: &Path, game: &Game, index: usize, event: Option<&(String, String, String)>) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let mut name = format!("{stem}-{index:02}");

    if let Some((_, label, difficulty)) = event.filter(|e| !e.1.is_empty()) {
        name.push('-');
        name.extend(label.chars().filter(|c| c.is_alphanumeric()));
        if !difficulty.is_empty() {
            name.push('-');
            name.extend(difficulty.chars().filter(|c| c.is_alphanumeric() || *c == '+'));
        }
    }

    path.with_file_name(format!("{name}{}", game.suffix))
}

pub fn split(
    game: &Game,
    path: &Path,
    short: bool,
    cancel: &AtomicBool,
    notify: &mut dyn FnMut(Event),
) -> Result<Vec<PathBuf>, String> {
    if game.id == "swtor" {
        return Err(format!("{} logs can't be split", game.name));
    }

    let fail = |e: std::io::Error| format!("{}: {e}", path.display());
    let file = File::open(path).map_err(fail)?;
    let size = file.metadata().map_or(1, |m| m.len().max(1));
    let mut reader = BufReader::with_capacity(1 << 20, file);

    let mut state = State::default();
    let mut headers: Vec<Vec<u8>> = Vec::new();
    let mut pieces: Vec<(PathBuf, i64, i64)> = Vec::new();
    let mut out: Option<BufWriter<File>> = None;
    let mut buf = Vec::new();
    let (mut read, mut shown) = (0u64, 0u64);

    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf).map_err(fail)?;
        if n == 0 || cancel.load(Ordering::Relaxed) {
            break;
        }
        read += n as u64;

        let text = String::from_utf8_lossy(&buf);
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            continue;
        }
        let parsed = line(game, text);

        if parsed.header {
            if parsed.reset {
                headers.clear();
            }
            headers.push(buf.clone());
        }

        let open = match &out {
            None => Some(None),
            Some(_) => split_here(game, &parsed, &mut state).then_some(parsed.event.as_ref()),
        };
        if let Some(event) = open {
            if let Some(mut done) = out.take() {
                done.flush().map_err(fail)?;
            }
            let next = piece(path, game, pieces.len() + 1, event);
            let mut writer = BufWriter::with_capacity(1 << 20, File::create(&next).map_err(fail)?);
            if !pieces.is_empty() {
                for header in headers.iter().filter(|h| !(parsed.header && **h == buf)) {
                    writer.write_all(header).map_err(fail)?;
                }
            }
            pieces.push((next, 0, 0));
            out = Some(writer);
        }

        if parsed.time > 0 {
            state.time = parsed.time;
            if let Some(last) = pieces.last_mut() {
                last.1 = if last.1 == 0 { parsed.time } else { last.1 };
                last.2 = parsed.time;
            }
        }
        if parsed.zone > 0 {
            state.zone = parsed.zone;
        }

        out.as_mut().map_or(Ok(()), |w| w.write_all(&buf)).map_err(fail)?;
        if read - shown > 4 << 20 {
            shown = read;
            notify(Event::Progress(read as f32 / size as f32));
        }
    }

    if let Some(mut done) = out {
        done.flush().map_err(fail)?;
    }
    notify(Event::Progress(1.0));

    if cancel.load(Ordering::Relaxed) || pieces.len() < 2 {
        for (p, ..) in &pieces {
            let _ = std::fs::remove_file(p);
        }
        return match cancel.load(Ordering::Relaxed) {
            true => Err("split cancelled".into()),
            false => Err(format!(
                "{}: nothing to split",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
        };
    }

    let (keep, drop): (Vec<_>, Vec<_>) = pieces
        .into_iter()
        .partition(|(_, start, end)| !short || *start == 0 || end - start >= SHORT);
    for (p, ..) in &drop {
        let _ = std::fs::remove_file(p);
    }

    Ok(keep.into_iter().map(|(p, ..)| p).collect())
}

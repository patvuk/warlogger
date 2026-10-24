use crate::upload::{Event, Fight, Stamp};
use memchr::{memchr, memchr_iter, memmem};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const CHUNK: usize = 8 << 20;
const FIELDS: usize = 16;
const FRIENDLY: i64 = 0x10;
const HOSTILE: i64 = 0x40;
const DAY: i64 = 86_400_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Version,
    Zone,
    EncounterStart,
    EncounterEnd,
    PhaseStart,
    PhaseEnd,
    KeyStart,
    KeyEnd,
    TowerStart,
    TowerEnd,
    ArenaStart,
    ArenaEnd,
    Fixed,
    Info,
    Damage,
    Tick,
    Death,
    Instakill,
    CastFailed,
    Absorbed,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mob {
    Player,
    Pet,
    Npc,
    None,
}

struct Potential {
    name: String,
    boss: Option<i64>,
    difficulty: Option<i64>,
    success: bool,
    pulls: i64,
    start: (i64, Stamp),
    end: (i64, Stamp),
}

struct Here<'a> {
    v: i64,
    m: i64,
    text: &'a str,
    year: i64,
    shift: i64,
}

impl Here<'_> {
    fn stamp(&self) -> Stamp {
        Stamp {
            text: self.text.to_owned(),
            year: self.year,
            shift: self.shift,
        }
    }
}

struct Fields<'a> {
    line: &'a str,
    spans: [(u32, u32); FIELDS],
    count: usize,
}

impl<'a> Fields<'a> {
    fn split(line: &'a str) -> Option<Self> {
        let bytes = line.as_bytes();
        let mut spans = [(0, 0); FIELDS];
        let (mut count, mut at) = (0, 0);

        while at <= bytes.len() {
            let piece = |from: usize| memchr(b',', &bytes[from..]).map_or(bytes.len(), |i| from + i);
            let mut end = piece(at);
            let mut span = (at, end);

            if end > at && bytes[at] == b'"' {
                if end - at >= 2 && bytes[end - 1] == b'"' {
                    span = (at + 1, end - 1);
                } else {
                    loop {
                        if end >= bytes.len() {
                            return None;
                        }
                        let next = piece(end + 1);
                        if next > end + 1 && bytes[next - 1] == b'"' {
                            span = (at + 1, next - 1);
                            end = next;
                            break;
                        }
                        end = next;
                    }
                }
            }

            if count == FIELDS {
                count += 1;
                break;
            }
            spans[count] = (span.0 as u32, span.1 as u32);
            count += 1;
            at = end + 1;
        }

        Some(Self { line, spans, count })
    }

    fn get(&self, i: usize) -> &'a str {
        match i < self.count.min(FIELDS) {
            true => &self.line[self.spans[i].0 as usize..self.spans[i].1 as usize],
            false => "",
        }
    }

    fn int(&self, i: usize) -> Option<i64> {
        int(self.get(i))
    }
}

fn int(text: &str) -> Option<i64> {
    let text = text.trim_start();
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let end = digits.bytes().position(|b| !b.is_ascii_digit()).unwrap_or(digits.len());
    digits[..end].parse::<i64>().ok().map(|v| v * sign)
}

fn hex(text: &str) -> i64 {
    let text = text.trim_start();
    let text = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    let end = text.bytes().position(|b| !b.is_ascii_hexdigit()).unwrap_or(text.len());
    i64::from_str_radix(&text[..end.min(15)], 16).unwrap_or(0)
}

fn kind(name: &str, f: &Fields, timeout: bool) -> Option<Kind> {
    let has = |s: &str| name.contains(s);
    let phase = |start| match timeout || f.count >= 4 && f.get(3).trim().parse::<f64>() == Ok(8.0) {
        true if start => Kind::PhaseStart,
        true => Kind::PhaseEnd,
        false if start => Kind::EncounterStart,
        false => Kind::EncounterEnd,
    };

    if has("SPELL_DURABILITY_DAMAGE") {
        return None;
    }
    let simple = match name {
        "COMBATANT_INFO" => Some(Kind::Info),
        "ZONE_CHANGE" => Some(Kind::Zone),
        "ENCOUNTER_START" => Some(phase(true)),
        "ENCOUNTER_END" => Some(phase(false)),
        "SPELL_SUMMON" | "SPELL_CREATE" | "SPELL_RESURRECT" => Some(Kind::Other),
        "UNIT_DIED" | "UNIT_DESTROYED" => Some(Kind::Death),
        "SPELL_INSTAKILL" => Some(Kind::Instakill),
        _ => None,
    };
    if simple.is_some() {
        return simple;
    }
    if has("_ENERGIZE") || has("_DRAIN") || has("_LEECH") {
        return Some(Kind::Other);
    }

    let special = match name {
        "SPELL_INTERRUPT"
        | "SPELL_DISPEL"
        | "SPELL_STOLEN"
        | "SPELL_AURA_BROKEN"
        | "SPELL_AURA_BROKEN_SPELL"
        | "SPELL_EXTRA_ATTACKS"
        | "SPELL_HEAL_ABSORBED" => Some(Kind::Other),
        "SPELL_ABSORBED" => Some(Kind::Absorbed),
        "CHALLENGE_MODE_START" => Some(Kind::KeyStart),
        "CHALLENGE_MODE_END" => Some(Kind::KeyEnd),
        "TOWER_START" => Some(Kind::TowerStart),
        "TOWER_END" => Some(Kind::TowerEnd),
        "ARENA_MATCH_START" => Some(Kind::ArenaStart),
        "ARENA_MATCH_END" => Some(Kind::ArenaEnd),
        "WORLD_MARKER_PLACED" | "WORLD_MARKER_REMOVED" => Some(Kind::Fixed),
        _ => None,
    };
    if special.is_some() {
        return special;
    }
    if has("ENVIRONMENTAL_") && f.count <= 9 {
        return None;
    }

    match name {
        "SPELL_DAMAGE_SUPPORT"
        | "SWING_DAMAGE_LANDED_SUPPORT"
        | "SPELL_PERIODIC_DAMAGE_SUPPORT"
        | "RANGE_DAMAGE_SUPPORT"
        | "SPELL_HEAL_SUPPORT"
        | "SPELL_PERIODIC_HEAL_SUPPORT" => return Some(Kind::Other),
        "SPELL_ABSORBED_SUPPORT" => return Some(Kind::Absorbed),
        _ => {}
    }
    if has("DAMAGE") || has("_MISSED") {
        return Some(if has("_PERIODIC") { Kind::Tick } else { Kind::Damage });
    }
    if has("_HEAL") {
        return Some(Kind::Other);
    }
    if has("_AURA_") {
        if f.count <= 12 {
            return None;
        }
        if matches!(
            name,
            "SPELL_AURA_APPLIED"
                | "SPELL_AURA_APPLIED_DOSE"
                | "SPELL_AURA_REMOVED"
                | "SPELL_AURA_REMOVED_DOSE"
                | "SPELL_AURA_REFRESH"
        ) {
            return Some(Kind::Other);
        }
    }

    match name {
        "SPELL_CAST_START" | "SPELL_CAST_SUCCESS" | "SPELL_EMPOWER_START" | "SPELL_EMPOWER_END" => Some(Kind::Other),
        "SPELL_CAST_FAILED" => Some(Kind::CastFailed),
        "COMBAT_LOG_VERSION" => Some(Kind::Version),
        "STAGGER_CLEAR" | "STAGGER_PREVENTED" => Some(Kind::Fixed),
        _ => None,
    }
}

fn mob(guid: &str, legacy: bool) -> (Mob, i64) {
    if legacy {
        let at = |from: usize, len: usize| guid.get(from..(from + len).min(guid.len())).map_or(0, hex);
        return match guid.get(4..5).map(hex) {
            Some(0 | 8) => (Mob::Player, at(11, 7)),
            Some(3 | 5 | 1) => (Mob::Npc, at(5, 5)),
            Some(4) => (Mob::Pet, at(5, 7)),
            _ => (Mob::None, 0),
        };
    }

    let mut parts = [""; 8];
    let mut count = 0;
    for part in guid.split('-') {
        if count < parts.len() {
            parts[count] = part;
        }
        count += 1;
    }

    let kind = match parts[0] {
        "Player" | "0000000000000000" => Mob::Player,
        "Pet" => Mob::Pet,
        _ => Mob::Npc,
    };
    let id = match kind {
        Mob::Player if count >= 3 => hex(parts[count.min(parts.len()) - 1]),
        Mob::Pet if count >= 7 => parts[6].get(3..10.min(parts[6].len())).map_or(0, hex),
        Mob::Npc if count >= 7 => int(parts[5]).unwrap_or(0),
        _ => 0,
    };
    (kind, id)
}

fn civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn dungeon(zone: i64) -> i64 {
    const IDS: [i64; 41] = [
        543, 542, 540, 547, 546, 545, 557, 558, 556, 555, 560, 269, 553, 554, 552, 585, 574, 575, 576, 578, 595, 599,
        600, 601, 602, 604, 608, 619, 632, 650, 658, 668, 643, 644, 645, 657, 670, 725, 755, 36, 33,
    ];
    match zone {
        938..=940 => 100_044 + zone - 938,
        _ => IDS.iter().position(|&z| z == zone).map_or(0, |i| 100_001 + i as i64),
    }
}

fn zone_timeout(classic: bool, zone: i64) -> i64 {
    let dungeon = matches!(
        zone,
        540 | 542
            | 543
            | 545
            | 546
            | 547
            | 552
            | 553
            | 554
            | 555
            | 556
            | 557
            | 558
            | 560
            | 269
            | 585
            | 574
            | 575
            | 576
            | 578
            | 595
            | 599
            | 600
            | 601
            | 602
            | 604
            | 608
            | 619
            | 632
            | 650
            | 658
            | 668
            | 643
            | 644
            | 645
            | 657
            | 670
            | 725
            | 755
            | 938
            | 939
            | 940
            | 36
            | 33
    );
    if classic && dungeon { 1_800_000 } else { 0 }
}

fn encounter_timeout(boss: Option<i64>) -> i64 {
    match boss {
        Some(660) => 1_800_000,
        Some(756 | 3197) => 180_000,
        Some(856) => 300_000,
        Some(1298 | 1291) => 60_000,
        _ => 90_000,
    }
}

fn key_name(id: Option<i64>) -> &'static str {
    match id.unwrap_or(0) {
        1456 => "Eye of Azshara",
        1466 => "Darkheart Thicket",
        1477 => "Halls of Valor",
        1458 => "Neltharion's Lair",
        1492 => "Maw of Souls",
        1493 => "Vault of the Wardens",
        1501 => "Black Rook Hold",
        1516 => "The Arcway",
        1544 => "Assault on Violet Hold",
        1571 => "Court of Stars",
        1677 => "Cathedral of Eternal Night",
        1651 => "Karazhan",
        1182 => "Auchindoun",
        1175 => "Bloodmaul Slag Mines",
        1279 => "Everbloom",
        1208 => "Grimrail Depot",
        1195 => "Iron Docks",
        1176 => "Shadowmoon Burial Grounds",
        1209 => "Skyreach",
        1358 => "Upper Blackrock Spire",
        _ => "Unknown Dungeon",
    }
}

#[derive(Default)]
struct Scanner {
    arena: Vec<Potential>,
    scanned: Vec<usize>,
    potential: Option<usize>,
    log_version: i64,
    game_version: i64,
    year: Option<i64>,
    previous: i64,
    shift: i64,
    start: Option<i64>,
    zone: i64,
    zone_name: String,
    zone_timeout: i64,
    in_combat: bool,
    in_dungeon: bool,
    in_tower: bool,
    map: Option<i64>,
    key: Option<i64>,
    stack: i64,
    threshold: i64,
    start_combat: i64,
    end_combat: i64,
    end_stamp: Stamp,
    last_encounter: i64,
    now: i64,
}

impl Scanner {
    fn new() -> Self {
        Self {
            log_version: 1,
            game_version: 1,
            threshold: 10_000,
            map: Some(0),
            now: now(),
            ..Self::default()
        }
    }

    fn time(&mut self, date: &str, clock: &str, millis: &str) -> Option<(i64, i64)> {
        let mut d = [0i64; 3];
        let mut n = 0;
        for part in date.split('/') {
            if n == 3 {
                return None;
            }
            d[n] = part.parse().ok()?;
            n += 1;
        }
        let mut c = [0i64; 3];
        for (i, part) in clock.splitn(3, ':').enumerate() {
            c[i] = part.parse().ok()?;
        }
        let exact = n == 3;
        let ms: i64 = match exact {
            true => millis.get(..3)?.parse().ok()?,
            false => millis.parse().ok()?,
        };
        let at = |year: i64| (civil(year, d[0], d[1]) * 24 + c[0]) * 3_600_000 + c[1] * 60_000 + c[2] * 1000 + ms;

        let now = self.now;
        let year = match (exact, self.year) {
            (true, _) => d[2],
            (false, Some(year)) => year,
            (false, None) => {
                let mut year = civil_year(now);
                while at(year) > now {
                    year -= 1;
                }
                self.year = Some(year);
                year
            }
        };

        let mut u = at(year);
        let mut year_used = year;
        if u < self.previous {
            let next = at(year + 1);
            if !exact && next <= now {
                self.year = Some(year + 1);
                year_used = year + 1;
                u = next;
                self.shift = 0;
            } else if self.previous - u >= 300_000 && !exact {
                while u + self.shift < self.previous {
                    self.shift += 3_600_000;
                }
            }
        }

        Some((u, year_used))
    }

    fn set_end(&mut self, here: &Here) {
        self.end_combat = here.m;
        self.end_stamp.text.clear();
        self.end_stamp.text.push_str(here.text);
        self.end_stamp.year = here.year;
        self.end_stamp.shift = here.shift;
    }

    fn enter(&mut self, here: &Here) {
        self.in_combat = true;
        self.start_combat = here.m;
        self.last_encounter = 0;
        if self.zone_timeout > 0 {
            self.threshold = self.zone_timeout;
        }

        let zoned = self.zone_timeout > 0;
        self.arena.push(Potential {
            name: if zoned {
                self.zone_name.clone()
            } else {
                "Unknown".into()
            },
            boss: Some(if zoned { dungeon(self.zone) } else { 0 }),
            difficulty: Some(if zoned { 10 } else { 0 }),
            success: zoned,
            pulls: 1,
            start: (here.v, here.stamp()),
            end: (here.v, here.stamp()),
        });
        self.potential = Some(self.arena.len() - 1);
    }

    fn push(&mut self) {
        let Some(p) = self.potential else { return };
        let end = (self.start.unwrap_or(0) + self.end_combat, self.end_stamp.clone());
        self.arena[p].pulls = 1;
        self.arena[p].end = end.clone();

        if let Some(&last) = self.scanned.last() {
            let (a, b) = (&self.arena[last], &self.arena[p]);
            if a.boss == b.boss && a.name == b.name && a.difficulty == b.difficulty && a.success == b.success {
                self.arena[last].pulls += 1;
                self.arena[last].end = end;
                return;
            }
        }
        self.scanned.push(p);
    }

    fn begin_encounter(&mut self, name: &str, boss: Option<i64>, difficulty: Option<i64>) {
        if let Some(p) = self.potential {
            let p = &mut self.arena[p];
            p.name = name.to_owned();
            p.boss = boss;
            p.difficulty = difficulty;
        }
    }

    fn end_encounter(&mut self, name: &str, boss: Option<i64>, difficulty: Option<i64>, won: bool) {
        if let Some(p) = self.potential {
            let p = &mut self.arena[p];
            if p.boss == Some(0) {
                p.name = name.to_owned();
                p.boss = boss;
                p.difficulty = difficulty;
            }
            p.success = boss.is_some() && boss == p.boss && won;
        }
    }

    fn participant(&mut self, name: &str, kind: Mob, reaction: i64) {
        if let Some(p) = self.potential
            && reaction == HOSTILE
            && kind == Mob::Npc
            && self.arena[p].name == "Unknown"
        {
            self.arena[p].name = name.to_owned();
        }
    }

    fn line(&mut self, line: &str) -> Result<(), String> {
        let Some(space) = memchr(b' ', line.as_bytes()) else {
            return Ok(());
        };
        if space == 0 {
            return Ok(());
        }
        let Some(gap) = memmem::find(&line.as_bytes()[space..], b"  ").map(|i| space + i) else {
            return Ok(());
        };
        if gap < space + 1 {
            return Ok(());
        }
        let clock = &line[space + 1..gap];
        let Some((clock, millis)) = clock.split_once('.') else {
            return Ok(());
        };
        if millis.contains('.') {
            return Ok(());
        }

        let Some(f) = Fields::split(&line[gap + 2..]) else {
            return Ok(());
        };
        let Some(kind) = kind(f.get(0), &f, self.zone_timeout > 0) else {
            return Ok(());
        };

        match kind {
            Kind::KeyStart => {
                let id = f.int(1).or(f.int(2));
                if id.is_some() && self.key == id {
                    return Ok(());
                }
                self.key = id;
            }
            Kind::KeyEnd => self.key = None,
            _ => {}
        }

        let Some((u, year)) = self.time(&line[..space], clock, millis) else {
            return Ok(());
        };
        let v = match kind {
            Kind::Fixed => self.previous + self.shift,
            _ => {
                self.previous = u;
                u + self.shift
            }
        };

        if kind == Kind::KeyEnd && self.log_version >= 7 && !self.in_dungeon || kind == Kind::TowerEnd && !self.in_tower
        {
            return Ok(());
        }

        if kind == Kind::Version {
            let version = f.int(1).unwrap_or(0);
            if f.int(3) != Some(1) {
                return Err("this log was created without Advanced Combat Logging".into());
            }
            self.log_version = match version {
                12.. if self.log_version < 16 => 16,
                11 if self.log_version < 15 => 15,
                10 if self.log_version < 14 => 14,
                9 if self.log_version < 13 => 13,
                _ => self.log_version,
            };
            if f.count >= 8 {
                self.game_version = match f.get(7) {
                    "2" => 2,
                    "18" => 7,
                    "5" => 3,
                    "11" => 4,
                    "14" => 5,
                    "19" => 6,
                    _ => self.game_version,
                };
            }
            return Ok(());
        }

        let start = *self.start.get_or_insert(v);
        let m = v - start;
        let here = Here {
            v,
            m,
            text: &line[..gap],
            year,
            shift: self.shift,
        };

        if kind == Kind::Zone {
            if f.count < 4 {
                return Ok(());
            }
            let zone = f.int(1).unwrap_or(-1);
            if self.zone_timeout > 0 && self.in_combat && zone != self.zone {
                self.in_combat = false;
                self.push();
            }
            self.zone = zone;
            self.zone_name = f.get(2).to_owned();
            self.zone_timeout = zone_timeout((2..=7).contains(&self.game_version), zone);
            return Ok(());
        }

        if self.in_combat {
            let idle = m - self.end_combat;
            if (self.in_dungeon || self.in_tower) && idle > self.threshold {
                self.in_combat = false;
                self.in_dungeon = false;
                self.in_tower = false;
                self.map = Some(0);
                self.threshold = 10_000;
            } else if self.stack > 0 && idle > self.threshold {
                self.in_combat = false;
                self.stack = 0;
                self.threshold = 10_000;
            } else if !self.in_dungeon && !self.in_tower && self.stack == 0 && idle > self.threshold
                || kind == Kind::EncounterStart && m - 2500 > self.start_combat
            {
                self.in_combat = false;
            } else if kind == Kind::KeyStart {
                self.in_combat = false;
                self.stack = 0;
            }
            if !self.in_combat {
                self.push();
            }
        }

        match kind {
            Kind::EncounterStart | Kind::EncounterEnd | Kind::PhaseStart | Kind::PhaseEnd => {
                let (boss, name, difficulty) = (f.int(1), f.get(2), f.int(3));
                if difficulty != Some(8) {
                    self.in_dungeon = false;
                    self.map = Some(0);
                }
                self.in_tower = false;

                let mut won = false;
                if kind == Kind::EncounterStart {
                    self.stack = 1;
                    if !self.in_combat {
                        self.enter(&here);
                    }
                    self.begin_encounter(name, boss, difficulty);
                    self.threshold = encounter_timeout(boss);
                } else if matches!(kind, Kind::EncounterEnd | Kind::PhaseEnd) {
                    won = f.get(5) == "1";
                    if kind == Kind::EncounterEnd {
                        self.stack = 0;
                    }
                }

                if self.in_combat {
                    self.set_end(&here);
                }
                if self.stack == 0 && matches!(kind, Kind::EncounterStart | Kind::EncounterEnd) {
                    self.last_encounter = m;
                    self.in_combat = false;
                    self.threshold = 10_000;
                    self.end_encounter(name, boss, difficulty, won);
                    self.push();
                }
                return Ok(());
            }
            Kind::KeyStart => {
                let (mut id, mut name) = (f.int(1), "");
                if id.is_none() {
                    name = f.get(1);
                    id = f.int(2);
                }
                self.enter(&here);
                self.set_end(&here);
                let label = if name.is_empty() { key_name(id) } else { name };
                self.begin_encounter(label, id, Some(8));
                self.in_dungeon = true;
                self.in_tower = false;
                self.map = id;
                self.threshold = 1_800_000;
                return Ok(());
            }
            Kind::KeyEnd => {
                let mut id = f.int(1);
                let won = f.int(2) == Some(1);
                id = match (self.map, id) {
                    (Some(2098), Some(2097)) => Some(2098),
                    (Some(2442), Some(2441)) => Some(2442),
                    (Some(2580), Some(2579)) => Some(2580),
                    _ => id,
                };
                if self.in_combat {
                    self.set_end(&here);
                }
                self.in_combat = false;
                self.in_dungeon = false;
                self.map = Some(0);
                self.threshold = 10_000;
                self.end_encounter(key_name(id), id, Some(8), won);
                self.push();
                return Ok(());
            }
            Kind::TowerStart => {
                let (id, name) = (f.int(1), f.get(2));
                self.enter(&here);
                self.set_end(&here);
                self.begin_encounter(name, id, Some(200));
                self.in_dungeon = false;
                self.map = Some(0);
                self.in_tower = true;
                self.threshold = 1_800_000;
                return Ok(());
            }
            Kind::TowerEnd => {
                let (id, name) = (f.int(1), f.get(2));
                let result = if f.count > 5 { f.int(5) } else { f.int(4) };
                if self.in_combat {
                    self.set_end(&here);
                }
                self.in_combat = false;
                self.threshold = 10_000;
                self.in_tower = false;
                self.end_encounter(name, id, Some(200), result == Some(1));
                self.push();
                return Ok(());
            }
            Kind::ArenaStart => {
                self.map = f.int(1);
                self.stack = 1;
                if !self.in_combat {
                    self.enter(&here);
                }
                self.set_end(&here);
                let zone = self.zone_name.clone();
                self.begin_encounter(&zone, None, Some(0));
                self.in_dungeon = false;
                self.in_tower = false;
                self.threshold = 1_800_000;
                return Ok(());
            }
            Kind::ArenaEnd => {
                self.stack = 0;
                self.last_encounter = m;
                self.in_combat = false;
                self.threshold = 10_000;
                let zone = self.zone_name.clone();
                self.end_encounter(&zone, self.map, Some(0), true);
                self.map = Some(0);
                self.push();
                return Ok(());
            }
            Kind::Fixed => return Ok(()),
            _ => {}
        }

        if f.count < 8 {
            return Ok(());
        }
        let mut shift = 0;
        if kind == Kind::Absorbed {
            shift = 8;
            let probe = f.get(9);
            if !probe.contains(':') && !probe.contains('-') {
                shift += 3;
            }
        }
        if kind == Kind::Info {
            return Ok(());
        }

        let (source, source_name, source_flags) = (f.get(shift + 1), f.get(shift + 2), hex(f.get(shift + 3)));
        let (target, target_name, target_flags) = (f.get(5), f.get(6), hex(f.get(7)));

        let modern = self.log_version >= 5 || source.contains('-') || source == "0000000000000000";
        if modern && self.log_version < 5 {
            self.log_version = 5;
        }
        if !modern {
            self.log_version = 1;
        }
        let (source_mob, source_id) = mob(source, !modern);
        let (target_mob, target_id) = mob(target, !modern);

        if kind == Kind::CastFailed {
            return Ok(());
        }

        let (a, b) = (source_flags & 0xF0, target_flags & 0xF0);
        self.participant(source_name, source_mob, a);
        self.participant(target_name, target_mob, b);

        let valid = |mob: Mob, id: i64| mob != Mob::Npc || matches!(id, 116_880 | 117_042 | 30_161);
        let outgoing = b != FRIENDLY && a == FRIENDLY && valid(source_mob, source_id);
        let incoming = a != FRIENDLY && b == FRIENDLY && valid(target_mob, target_id);

        let extends = match kind {
            Kind::Damage | Kind::Tick => {
                let hit = source_id != 0 && target_id != 0 && kind == Kind::Damage && (outgoing || incoming);
                if hit && !self.in_combat && (self.last_encounter == 0 || outgoing || m - 3000 >= self.last_encounter) {
                    self.enter(&here);
                }
                hit
            }
            Kind::Death => b != FRIENDLY,
            Kind::Instakill => b == FRIENDLY && a != FRIENDLY,
            _ => false,
        };
        if extends && self.in_combat {
            self.set_end(&here);
        }
        Ok(())
    }

    fn finish(mut self) -> Vec<Fight> {
        self.push();
        let scanned = std::mem::take(&mut self.scanned);
        scanned
            .into_iter()
            .map(|i| {
                let p = &self.arena[i];
                Fight {
                    name: p.name.clone(),
                    start: p.start.0,
                    end: p.end.0,
                    boss: p.boss.unwrap_or(0),
                    difficulty: p.difficulty.unwrap_or(0),
                    success: p.success,
                    pulls: p.pulls,
                    stamps: Some((p.start.1.clone(), p.end.1.clone())),
                }
            })
            .collect()
    }
}

fn civil_year(ms: i64) -> i64 {
    let days = ms.div_euclid(DAY) + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    yoe + era * 400 + i64::from(month <= 2)
}

pub fn supported(game: &crate::game::Game) -> bool {
    game.prefix == "WoWCombatLog" && std::env::var_os("WARLOGGER_PARSER_SCAN").is_none()
}

pub fn scan(path: &Path, cancel: &AtomicBool, notify: &mut dyn FnMut(Event)) -> Result<Vec<Fight>, String> {
    let fail = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut file = File::open(path).map_err(fail)?;
    let size = file.metadata().map_or(1, |m| m.len().max(1));

    let mut scanner = Scanner::new();
    let mut buf = vec![0u8; CHUNK];
    let (mut filled, mut read) = (0, 0u64);

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("scan cancelled".into());
        }
        let n = file.read(&mut buf[filled..]).map_err(fail)?;
        filled += n;
        read += n as u64;
        let eof = n == 0;

        let mut from = 0;
        for nl in memchr_iter(b'\n', &buf[..filled]) {
            let raw = &buf[from..nl];
            let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
            from = nl + 1;
            if !raw.is_empty() {
                scanner.line(&String::from_utf8_lossy(raw))?;
            }
        }
        if eof {
            let raw = &buf[from..filled];
            if !raw.is_empty() {
                scanner.line(&String::from_utf8_lossy(raw))?;
            }
            break;
        }

        buf.copy_within(from..filled, 0);
        filled -= from;
        if filled == buf.len() {
            buf.resize(buf.len() * 2, 0);
        }
        notify(Event::Progress(read as f32 / size as f32));
    }

    notify(Event::Progress(1.0));
    Ok(scanner.finish())
}

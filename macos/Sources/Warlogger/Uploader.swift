import Foundation

enum Event {
    case step(String)
    case progress(Double)
    case report(String)
}

struct Target {
    var region: Any?
    var guild: Any?
    var visibility: Any?
    var description = ""
}

private struct Part {
    var lines: [String]
    var span: (Int64, Int64)
    var eof: Bool
}

private final class Reader {
    private let handle: FileHandle
    private let latin1: Bool
    private(set) var pos: Int64 = 0
    private let chunk = 4 << 20

    init(_ url: URL, latin1: Bool) throws {
        handle = try FileHandle(forReadingFrom: url)
        self.latin1 = latin1
    }

    deinit { try? handle.close() }

    var length: Int64 { Int64((try? handle.seekToEnd()) ?? 0) }

    func next(flush: Bool) throws -> Part {
        let start = pos
        try handle.seek(toOffset: UInt64(start))
        var data = try handle.read(upToCount: chunk) ?? Data()

        let cut: Int
        if let i = data.lastIndex(of: 10) {
            cut = i - data.startIndex + 1
        } else {
            cut = flush || data.count == chunk ? data.count : 0
        }
        data = data.prefix(cut)
        pos = start + Int64(cut)

        let lines = data.split(separator: 10).compactMap { raw -> String? in
            let line = raw.last == 13 ? raw.dropLast() : raw
            guard !line.isEmpty else { return nil }
            return latin1 ? String(line.map { Character(Unicode.Scalar($0)) }) : String(decoding: line, as: UTF8.self)
        }
        return Part(lines: lines, span: (start, pos), eof: pos >= length)
    }
}

final class Uploader {
    let game: Game
    let site: Site
    let account: Account
    private let engine: Engine
    private var code = ""
    private var segment = 1
    private var live = false

    init(game: Game, email: String, password: String, notify: (Event) -> Void) async throws {
        self.game = game
        site = Site(game: game)
        await site.prepare()
        notify(.step("Signing in to \(game.host)"))
        account = try await site.login(email: email, password: password)
        notify(.step("Loading the site's parser"))
        engine = try Engine(scripts: try await site.parser())
    }

    private func begin(_ target: Target, file: String, live: Bool) async throws {
        try engine.reset()
        let guildRegion = account.guilds.first { $0.key == target.guild.map { "\($0)" } }?.region
        code = try await site.create(Report(file: file, guild: target.guild, region: guildRegion ?? target.region,
                                            visibility: target.visibility, description: target.description,
                                            parser: engine.version, mode: live ? 1 : 2))
        try engine.reportCode(code)
        segment = 1
        self.live = live
    }

    private func part(_ part: Part, region: Any?, file: String, push: Bool) async throws -> Bool {
        if part.lines.isEmpty && !push { return false }
        if !part.lines.isEmpty { try engine.parse(part.lines, region: region, file: file, span: part.span) }

        let fights = try engine.fights(push: push)
        guard fights.count > 0 else { return false }

        let (master, players) = try engine.master(code)
        try await site.master(code, segment: segment, players: players, zip: zip(master))

        let events = zip(try engine.fightsText())
        let next = try await site.segment(code, parameters: [
            "startTime": fights.start, "endTime": fights.end, "mythic": fights.mythic, "isLiveLog": live,
            "isRealTime": false, "inProgressEventCount": 0, "segmentId": segment,
        ], zip: events)
        if next > 0 { segment = next }

        try engine.clear()
        return true
    }

    private func finish<T>(_ body: () async throws -> T) async throws -> T {
        do {
            let value = try await body()
            try engine.reset()
            try await site.terminate(code)
            return value
        } catch {
            try? engine.reset()
            try? await site.terminate(code)
            throw error
        }
    }

    func upload(_ url: URL, target: Target, cancelled: () -> Bool, notify: (Event) -> Void) async throws -> String {
        let reader = try Reader(url, latin1: game.latin1)
        let size = Double(max(reader.length, 1))

        notify(.step("Uploading \(url.lastPathComponent)"))
        try await begin(target, file: url.lastPathComponent, live: false)
        notify(.report(site.reportURL(code)))

        let fights = try await finish { () async throws -> Int in
            if let date = game.startDate(url) { try engine.startDate(date) }
            var fights = 0
            while !cancelled() {
                let next = try reader.next(flush: true)
                fights += try await part(next, region: target.region, file: url.path, push: next.eof) ? 1 : 0
                notify(.progress(Double(reader.pos) / size))
                if next.eof { break }
            }
            return fights
        }

        guard fights > 0 else { throw SiteError(message: "\(url.lastPathComponent): no fights found") }
        return site.reportURL(code)
    }

    func follow(_ dir: URL, target: Target, all: Bool, cancelled: () -> Bool, notify: (Event) -> Void) async throws -> String {
        try await begin(target, file: "live.log", live: true)
        let url = site.reportURL(code)
        notify(.report(url))

        try await finish {
            if !all { try engine.liveStart(nowMs()) }
            var current: (URL, Reader)?
            var pushed = true
            var waiting = false

            while true {
                let stop = cancelled()
                if let log = game.logs(in: dir).first, Date().timeIntervalSince(log.modified) < 6 * 3600, current?.0 != log.url {
                    notify(.step("Watching \(log.url.lastPathComponent)"))
                    if let date = game.startDate(log.url) { try engine.startDate(date) }
                    current = (log.url, try Reader(log.url, latin1: game.latin1))
                }

                guard let (path, reader) = current else {
                    if stop { return }
                    if !waiting { notify(.step("Waiting for a \(game.name) log in \(dir.path)")) }
                    waiting = true
                    try await Task.sleep(nanoseconds: 1_000_000_000)
                    continue
                }

                let next = try reader.next(flush: stop)
                let fresh = !next.lines.isEmpty
                let modified = (try? path.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                let idle = Date().timeIntervalSince(modified) > 120

                pushed = pushed && !fresh
                let push = stop || (idle && !pushed)
                if try await part(next, region: target.region, file: path.path, push: push) {
                    notify(.step("Uploaded new fights"))
                }
                pushed = pushed || push

                if stop { return }
                if !fresh { try await Task.sleep(nanoseconds: 1_000_000_000) }
            }
        }
        return url
    }
}

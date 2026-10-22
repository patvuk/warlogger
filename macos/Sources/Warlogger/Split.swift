import Foundation

private let gap: Int64 = 4 * 3600 * 1000

private struct Line {
    var time: Int64 = 0
    var zone: Int64 = 0
    var force = false
    var header = false
    var reset = false
    var event: (kind: String, label: String, difficulty: String)?
}

private struct State {
    var time: Int64 = 0
    var zone: Int64 = 0
    var key = false
    var last: [String]?
}

private func civil(_ y: Int64, _ m: Int64, _ d: Int64) -> Int64 {
    let y = m <= 2 ? y - 1 : y
    let era = (y >= 0 ? y : y - 399) / 400
    let yoe = y - era * 400
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1
    return era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy
}

private func stamp(_ p: [Int64]) -> Int64 {
    guard p.count >= 6 else { return 0 }
    return ((civil(p[0], p[1], p[2]) * 24 + p[3]) * 60 + p[4]) * 60_000 + p[5] * 1000
}

private func numbers(_ text: Substring) -> [Int64] {
    Array(text.split { !$0.isASCII || !$0.isNumber }.prefix(7).map { Int64($0) ?? 0 })
}

private func wow(_ game: Game, _ text: String) -> Line {
    guard let sep = text.range(of: "  ") else { return Line() }
    let when = text[..<sep.lowerBound], rest = text[sep.upperBound...]
    let n = numbers(when)
    var line = Line(header: text.contains("COMBAT_LOG_VERSION"))

    if when.split(separator: " ").first?.filter({ $0 == "/" }).count == 2, n.count >= 6 {
        line.time = stamp([n[2], n[0], n[1], n[3], n[4], n[5]])
    } else if n.count >= 5 {
        line.time = stamp([2000, n[0], n[1], n[2], n[3], n[4]])
    }

    let kind = String(rest.prefix { $0 != "," })
    guard game.id == "wow" else { return line }
    switch kind {
    case "ENCOUNTER_START", "CHALLENGE_MODE_START":
        guard let open = rest.firstIndex(of: "\""), let close = rest.lastIndex(of: "\""), open < close else { break }
        let label = String(rest[rest.index(after: open)..<close])
        let fields = rest[rest.index(after: close)...].split(separator: ",", omittingEmptySubsequences: false)
        let difficulty = kind == "ENCOUNTER_START"
            ? game.difficulty(fields.count > 1 ? Int64(fields[1]) ?? 0 : 0)
            : fields.count > 3 ? String(fields[3]) : ""
        line.event = (kind, label, difficulty)
    case "CHALLENGE_MODE_END":
        line.event = (kind, "", "")
    default:
        break
    }
    return line
}

private func parse(_ game: Game, _ text: String) -> Line {
    let fields = text.split(maxSplits: 4, omittingEmptySubsequences: false) { $0 == "|" || (game.id == "eso" && $0 == ",") }
    let field = { (i: Int) in i < fields.count ? String(fields[i]) : "" }

    switch game.id {
    case "ff":
        return Line(time: stamp(numbers(Substring(field(1)))),
                    zone: field(0) == "01" ? Int64(field(2), radix: 16) ?? 0 : 0,
                    header: ["250", "253"].contains(field(0)) || field(0) == "251" && text.contains("DeucalionClient"),
                    reset: field(0) == "253")
    case "eso":
        return Line(force: field(1) == "BEGIN_LOG")
    case "fellowship":
        return Line(time: stamp(numbers(Substring(field(0)))),
                    zone: field(1) == "ZONE_CHANGE" ? Int64(field(3)) ?? 0 : 0,
                    header: field(1) == "LOGGING_STARTED", reset: field(1) == "LOGGING_STARTED")
    default:
        return wow(game, text)
    }
}

private func splitHere(_ game: Game, _ line: Line, _ state: inout State) -> Bool {
    if let event = line.event {
        let key = [event.label, event.difficulty]
        switch event.kind {
        case "CHALLENGE_MODE_END":
            state.key = false
            return false
        case "CHALLENGE_MODE_START":
            state.key = true
            return true
        default:
            if state.key {
                let left = event.difficulty != game.difficulty(8)
                if left { state.key = false; state.last = key }
                return left
            }
            let changed = state.last != key
            state.last = key
            return changed
        }
    }

    let gapped = state.time > 0 && line.time > state.time + gap
    switch game.id {
    case "eso": return line.force
    case "ff", "fellowship": return gapped || state.zone > 0 && line.zone > 0 && state.zone != line.zone
    default: return gapped
    }
}

private func piece(_ url: URL, _ game: Game, _ index: Int, _ event: (kind: String, label: String, difficulty: String)?) -> URL {
    var name = url.deletingPathExtension().lastPathComponent + String(format: "-%02d", index)
    if let event, !event.label.isEmpty {
        name += "-" + event.label.filter { $0.isLetter || $0.isNumber }
        if !event.difficulty.isEmpty { name += "-" + event.difficulty.filter { $0.isLetter || $0.isNumber || $0 == "+" } }
    }
    return url.deletingLastPathComponent().appendingPathComponent(name + game.suffix)
}

private final class Writer {
    let handle: FileHandle
    var buffer = Data()

    init(_ url: URL) throws {
        guard FileManager.default.createFile(atPath: url.path, contents: nil) else {
            throw SiteError(message: "Can't create \(url.lastPathComponent)")
        }
        handle = try FileHandle(forWritingTo: url)
    }

    func write(_ data: Data) throws {
        buffer.append(data)
        if buffer.count > 1 << 20 { try flush() }
    }

    func flush() throws {
        try handle.write(contentsOf: buffer)
        buffer.removeAll(keepingCapacity: true)
    }

    func close() throws {
        try flush()
        try handle.close()
    }
}

func split(_ game: Game, _ url: URL, short: Bool, cancelled: () -> Bool, notify: (Event) -> Void) throws -> [URL] {
    guard game.id != "swtor" else { throw SiteError(message: "\(game.name) logs can't be split") }

    let input = try FileHandle(forReadingFrom: url)
    defer { try? input.close() }
    let size = Double(max((try? input.seekToEnd()) ?? 1, 1))
    try input.seek(toOffset: 0)

    var state = State()
    var headers: [Data] = []
    var pieces: [(url: URL, start: Int64, end: Int64)] = []
    var out: Writer?
    var rest = Data()
    var read = 0

    func handle(_ raw: Data) throws {
        var text = String(decoding: raw, as: UTF8.self)
        while text.hasSuffix("\n") || text.hasSuffix("\r") { text.removeLast() }
        guard !text.isEmpty else { return }
        let line = parse(game, text)

        if line.header {
            if line.reset { headers.removeAll() }
            headers.append(raw)
        }

        if out == nil || splitHere(game, line, &state) {
            try out?.close()
            let next = piece(url, game, pieces.count + 1, out == nil ? nil : line.event)
            let writer = try Writer(next)
            if !pieces.isEmpty {
                for header in headers where !(line.header && header == raw) { try writer.write(header) }
            }
            pieces.append((next, 0, 0))
            out = writer
        }

        if line.time > 0 {
            state.time = line.time
            pieces[pieces.count - 1].start = pieces[pieces.count - 1].start == 0 ? line.time : pieces[pieces.count - 1].start
            pieces[pieces.count - 1].end = line.time
        }
        if line.zone > 0 { state.zone = line.zone }
        try out?.write(raw)
    }

    while !cancelled() {
        let chunk = try input.read(upToCount: 4 << 20) ?? Data()
        if chunk.isEmpty { break }
        read += chunk.count
        rest.append(chunk)

        var start = rest.startIndex
        while let nl = rest[start...].firstIndex(of: 10) {
            try handle(rest[start...nl])
            start = nl + 1
        }
        rest = Data(rest[start...])
        notify(.progress(Double(read) / size))
    }
    if !rest.isEmpty && !cancelled() { try handle(rest) }
    try out?.close()

    let remove = { (list: [URL]) in list.forEach { try? FileManager.default.removeItem(at: $0) } }
    if cancelled() || pieces.count < 2 {
        remove(pieces.map(\.url))
        throw SiteError(message: cancelled() ? "Split cancelled" : "\(url.lastPathComponent): nothing to split")
    }

    let keep = pieces.filter { !short || $0.start == 0 || $0.end - $0.start >= shortFight }
    remove(pieces.filter { p in !keep.contains { $0.url == p.url } }.map(\.url))
    return keep.map(\.url)
}

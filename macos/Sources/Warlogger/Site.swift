import Foundation

struct Choice: Hashable {
    let value: AnyHashable?
    let label: String
    let region: AnyHashable?

    var key: String { value.map { "\($0)" } ?? "" }
}

struct Account {
    var name = ""
    var guilds: [Choice] = []
    var regions: [Choice] = []
    var visibilities: [Choice] = []
}

struct Report {
    var file: String
    var guild: Any?
    var region: Any?
    var visibility: Any?
    var description: String
    var parser: Int
    var mode: Int
}

struct SiteError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

func nowMs() -> Int64 { Int64(Date().timeIntervalSince1970 * 1000) }

final class Site {
    let game: Game
    private var version = "9.6.158"
    private let session: URLSession

    init(game: Game) {
        self.game = game
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 300
        session = URLSession(configuration: config)
    }

    func reportURL(_ code: String) -> String { "\(game.base)/reports/\(code)" }

    private var agent: String {
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) ArchonApp/\(version) Chrome/138.0.7204.251 Electron/37.7.0 Safari/537.36"
    }

    func prepare() async {
        var request = URLRequest(url: URL(string: "https://api.github.com/repos/RPGLogs/Uploaders-archon/releases/latest")!)
        request.setValue("warlogger", forHTTPHeaderField: "User-Agent")
        guard let (data, _) = try? await URLSession.shared.data(for: request),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let tag = json["tag_name"] as? String, !tag.isEmpty else { return }
        version = tag.hasPrefix("v") ? String(tag.dropFirst()) : tag
    }

    private func message(_ data: Data) -> String {
        let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        for key in ["message", "error", "reason"] {
            if let text = json?[key] as? String, !text.isEmpty { return text }
        }
        return String(String(decoding: data, as: UTF8.self).prefix(300))
    }

    private func fetch(_ method: String, _ url: String, type: String? = nil, body: Data? = nil) async throws -> Data {
        var request = URLRequest(url: URL(string: url)!)
        request.httpMethod = method
        request.httpBody = body
        request.setValue(agent, forHTTPHeaderField: "User-Agent")
        request.setValue("application/json, text/html, */*", forHTTPHeaderField: "Accept")
        if let type { request.setValue(type, forHTTPHeaderField: "Content-Type") }

        let xsrf = session.configuration.httpCookieStorage?.cookies(for: request.url!)?.first { $0.name == "XSRF-TOKEN" }
        if let token = xsrf?.value.removingPercentEncoding { request.setValue(token, forHTTPHeaderField: "X-XSRF-TOKEN") }

        for attempt in 0...4 {
            let delay = UInt64(1 << attempt) * 1_000_000_000
            do {
                let (data, response) = try await session.data(for: request)
                let status = (response as? HTTPURLResponse)?.statusCode ?? 0
                switch status {
                case 200..<300: return data
                case 429, 500...: if attempt < 4 { try await Task.sleep(nanoseconds: delay); continue }
                case 401, 419: throw SiteError(message: "Not signed in to \(game.host)")
                default: break
                }
                throw SiteError(message: "\(message(data)) (HTTP \(status))")
            } catch let error as URLError where attempt < 4 {
                _ = error
                try await Task.sleep(nanoseconds: delay)
            }
        }
        throw SiteError(message: "\(url): gave up")
    }

    private func json(_ method: String, _ path: String, _ body: [String: Any?]? = nil) async throws -> [String: Any] {
        let data = try body.map { try JSONSerialization.data(withJSONObject: $0.mapValues { $0 ?? NSNull() }) }
        let out = try await fetch(method, game.base + path, type: body == nil ? nil : "application/json", body: data)
        return (try? JSONSerialization.jsonObject(with: out) as? [String: Any]) ?? [:]
    }

    private func choices(_ value: Any?) -> [Choice] {
        ((value as? [[String: Any]]) ?? []).map {
            Choice(value: $0["value"] as? AnyHashable, label: $0["label"] as? String ?? "", region: $0["regionId"] as? AnyHashable)
        }
    }

    func login(email: String, password: String) async throws -> Account {
        let v = try await json("POST", "/desktop-client/log-in", ["email": email, "password": password, "version": version])
        guard let user = v["user"] as? [String: Any] else { throw SiteError(message: "Sign in to \(game.host) failed") }

        let pick = { (key: String) in self.choices(v[key] ?? user[key]) }
        return Account(name: user["userName"] as? String ?? email, guilds: pick("guildSelectItems"),
                       regions: pick("regionOrServerSelectItems"), visibilities: pick("reportVisibilitySelectItems"))
    }

    func parser() async throws -> [String] {
        let url = "\(game.base)/desktop-client/parser?id=1&ts=\(nowMs())&gameContentDetectionEnabled=false&metersEnabled=false&liveFightDataEnabled=false&gameVersionId="
        let html = String(decoding: try await fetch("GET", url), as: UTF8.self)
        let pattern = try NSRegularExpression(pattern: "<script([^>]*)>([\\s\\S]*?)</script>")

        var scripts: [String] = []
        for match in pattern.matches(in: html, range: NSRange(html.startIndex..., in: html)) {
            let tag = String(html[Range(match.range(at: 1), in: html)!])
            let code = String(html[Range(match.range(at: 2), in: html)!])
            if let type = attribute(tag, "type"), !type.contains("javascript") { continue }

            if let src = attribute(tag, "src") {
                guard src.contains("rpglogs.") || src.hasPrefix("/") else { continue }
                let full = src.hasPrefix("/") ? game.base + src : src
                scripts.append(String(decoding: try await fetch("GET", full), as: UTF8.self))
            } else if !code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                scripts.append(code)
            }
        }

        guard !scripts.isEmpty else { throw SiteError(message: "No parser scripts found") }
        return scripts
    }

    private func attribute(_ tag: String, _ name: String) -> String? {
        guard let range = tag.range(of: "\(name)=") else { return nil }
        let rest = tag[range.upperBound...]
        guard let quote = rest.first, quote == "\"" || quote == "'" else { return nil }
        let value = rest.dropFirst()
        return value.firstIndex(of: quote).map { String(value[..<$0]).replacingOccurrences(of: "&amp;", with: "&") }
    }

    func create(_ r: Report) async throws -> String {
        let now = nowMs()
        let v = try await json("POST", "/desktop-client/create-report", [
            "clientVersion": version, "parserVersion": r.parser, "startTime": now, "endTime": now,
            "guildId": r.guild, "fileName": r.file, "serverOrRegion": r.region, "visibility": r.visibility,
            "reportTagId": nil, "description": r.description, "logMode": r.mode,
        ])
        guard let code = v["code"] as? String else { throw SiteError(message: "create-report returned no code") }
        return code
    }

    private func multipart(_ fields: [(String, String)], _ file: Data) -> (String, Data) {
        let boundary = "----WarloggerBoundary\(nowMs())"
        var body = Data()
        for (name, value) in fields {
            body.append("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(name)\"\r\n\r\n\(value)\r\n".data(using: .utf8)!)
        }
        body.append("--\(boundary)\r\nContent-Disposition: form-data; name=\"logfile\"; filename=\"blob\"\r\nContent-Type: application/zip\r\n\r\n".data(using: .utf8)!)
        body.append(file)
        body.append("\r\n--\(boundary)--\r\n".data(using: .utf8)!)
        return ("multipart/form-data; boundary=\(boundary)", body)
    }

    func master(_ code: String, segment: Int, players: String, zip: Data) async throws {
        let (type, body) = multipart([("segmentId", "\(segment)"), ("isRealTime", "false"), ("players", players)], zip)
        _ = try await fetch("POST", "\(game.base)/desktop-client/set-report-master-table/\(code)", type: type, body: body)
    }

    func segment(_ code: String, parameters: [String: Any], zip: Data) async throws -> Int {
        let json = String(decoding: try JSONSerialization.data(withJSONObject: parameters), as: UTF8.self)
        let (type, body) = multipart([("parameters", json)], zip)
        let out = try await fetch("POST", "\(game.base)/desktop-client/add-report-segment/\(code)", type: type, body: body)
        let v = (try? JSONSerialization.jsonObject(with: out) as? [String: Any]) ?? [:]
        return (v["nextSegmentId"] as? NSNumber)?.intValue ?? 0
    }

    func terminate(_ code: String) async throws {
        _ = try await fetch("POST", "\(game.base)/desktop-client/terminate-report/\(code)")
    }
}

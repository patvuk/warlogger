import Foundation
import JavaScriptCore

struct Summary {
    let count: Int
    let start: Int64
    let end: Int64
    let mythic: Int
}

final class Engine {
    private let context: JSContext
    private(set) var version = 0
    private var failure: String?

    init(scripts: [String]) throws {
        guard let context = JSContext(),
              let url = Bundle.main.url(forResource: "host", withExtension: "js"),
              let host = try? String(contentsOf: url, encoding: .utf8)
        else { throw SiteError(message: "JavaScript host is missing from the app bundle") }

        self.context = context
        context.exceptionHandler = { [weak self] _, value in self?.failure = value?.toString() }
        context.evaluateScript(host)
        for script in scripts {
            context.evaluateScript(script)
            failure = nil
        }

        pump()
        guard context.evaluateScript("__listeners.length > 0").toBool() else {
            throw SiteError(message: "The parser page did not register a message handler")
        }

        try ask("get-parser-version")
        version = Int(context.evaluateScript("__version()").toInt32())
    }

    private func pump() {
        while context.evaluateScript("__timers.length > 0").toBool() {
            context.evaluateScript("__drain()")
        }
    }

    private func eval(_ code: String) throws -> JSValue {
        failure = nil
        let value = context.evaluateScript(code)!
        if let failure { throw SiteError(message: failure) }
        return value
    }

    private func ask(_ name: String, _ fields: [String: Any] = [:]) throws {
        var message = fields
        message["message"] = name
        message["id"] = 1

        failure = nil
        context.objectForKeyedSubscript("__send").call(withArguments: [message])
        if let failure { throw SiteError(message: "\(name): \(failure)") }

        pump()
        guard try eval("__take('\(name)-completed')").toBool() else {
            throw SiteError(message: "The parser did not answer \(name)")
        }
    }

    func reset() throws {
        try ask("clear-fights")
        try ask("clear-state")
    }

    func startDate(_ date: String) throws { try ask("set-start-date", ["startDate": date]) }

    func reportCode(_ code: String) throws { try ask("set-report-code", ["reportCode": code]) }

    func liveStart(_ ms: Int64) throws { try ask("set-live-logging-start-time", ["startTime": ms]) }

    func parse(_ lines: [String], region: Any?, file: String, span: (Int64, Int64),
               raids: [(Int64, Int64)] = [], scanning: Bool = false) throws {
        try ask("parse-lines", [
            "lines": lines, "selectedRegion": (region as? NSNumber) ?? 0,
            "raidsToUpload": raids.map { ["start": $0.0, "end": $0.1] }, "scanning": scanning,
            "logFilePosition": ["filePath": file, "startingPosition": span.0, "currentPosition": span.1],
        ])

        let error = try eval("__parseError()").toString() ?? ""
        if !error.isEmpty { throw SiteError(message: error) }
    }

    func fights(push: Bool) throws -> Summary {
        try ask("collect-fights", ["pushFightIfNeeded": push, "scanningOnly": false])
        let s = try eval("__summary()").toString() ?? "{}"
        let v = (try? JSONSerialization.jsonObject(with: Data(s.utf8)) as? [String: NSNumber]) ?? [:]
        return Summary(count: v["count"]?.intValue ?? 0, start: v["start"]?.int64Value ?? 0,
                       end: v["end"]?.int64Value ?? 0, mythic: v["mythic"]?.intValue ?? 0)
    }

    func scanned() throws -> [Fight] {
        try ask("collect-fights", ["pushFightIfNeeded": true, "scanningOnly": true])
        try ask("collect-scanned-raids")
        let s = try eval("__scanned()").toString() ?? "[]"
        let list = (try? JSONSerialization.jsonObject(with: Data(s.utf8)) as? [[String: Any]]) ?? []
        let num = { (v: [String: Any], k: String) in (v[k] as? NSNumber)?.int64Value ?? 0 }
        return list.map {
            Fight(name: $0["name"] as? String ?? "Unknown", start: num($0, "start"), end: num($0, "end"), boss: num($0, "boss"),
                  difficulty: num($0, "difficulty"), success: $0["success"] as? Bool ?? false, pulls: num($0, "pulls"))
        }
    }

    func fightsText() throws -> String { try eval("__fightsText()").toString() ?? "" }

    func master(_ code: String) throws -> (String, String) {
        try ask("collect-master-info", ["reportCode": code])
        let error = try eval("__masterError()").toString() ?? ""
        if !error.isEmpty { throw SiteError(message: error) }
        return (try eval("__masterText()").toString() ?? "", try eval("__players()").toString() ?? "")
    }

    func clear() throws {
        try ask("clear-fights")
        _ = try eval("__release()")
        JSGarbageCollect(context.jsGlobalContextRef)
    }
}

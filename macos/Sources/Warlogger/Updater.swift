import AppKit

struct Update {
    let version: String
    let url: URL
    let size: Int
}

enum Updater {
    private static let latest = URL(string: "https://api.github.com/repos/patvuk/warlogger/releases/latest")!

    static var current: String { Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0" }

    private static func parts(_ version: String) -> [Int] {
        version.drop { $0 == "v" }.split(separator: ".").map { Int($0) ?? 0 }
    }

    static func check() async -> Update? {
        var request = URLRequest(url: latest)
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        guard let (data, response) = try? await URLSession.shared.data(for: request),
              (response as? HTTPURLResponse)?.statusCode == 200,
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let version = (json["tag_name"] as? String).map({ String($0.drop { $0 == "v" }) }),
              parts(current).lexicographicallyPrecedes(parts(version)),
              let asset = (json["assets"] as? [[String: Any]])?.first(where: { ($0["name"] as? String)?.hasSuffix("-macos-universal.zip") == true }),
              let link = asset["browser_download_url"] as? String, let url = URL(string: link)
        else { return nil }
        return Update(version: version, url: url, size: (asset["size"] as? NSNumber)?.intValue ?? 0)
    }

    private static func run(_ tool: String, _ args: [String]) throws {
        let process = try Process.run(URL(fileURLWithPath: tool), arguments: args)
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw SiteError(message: "\(tool) failed") }
    }

    static func install(_ update: Update) async throws {
        let (file, _) = try await URLSession.shared.download(from: update.url)
        let size = (try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        guard size == update.size else { throw SiteError(message: "Download is \(size) bytes, expected \(update.size)") }

        let fm = FileManager.default
        let dir = fm.temporaryDirectory.appendingPathComponent("warlogger-\(update.version)")
        try? fm.removeItem(at: dir)
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        try run("/usr/bin/ditto", ["-x", "-k", file.path, dir.path])

        let app = Bundle.main.bundleURL
        let old = dir.appendingPathComponent("old.app")
        try fm.moveItem(at: app, to: old)
        do {
            try fm.moveItem(at: dir.appendingPathComponent("Warlogger.app"), to: app)
        } catch {
            try? fm.moveItem(at: old, to: app)
            throw error
        }

        try Process.run(URL(fileURLWithPath: "/bin/sh"), arguments: ["-c", "sleep 1; open \"$0\"", app.path])
        await MainActor.run { NSApp.terminate(nil) }
    }
}

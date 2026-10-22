import Foundation

struct Game: Identifiable, Hashable {
    let id: String
    let name: String
    let host: String
    let prefix: String
    let suffix: String
    let latin1: Bool
    let dirs: [String]

    var base: String { "https://\(host)" }
    var site: String { host.split(separator: ".", maxSplits: 1).last.map(String.init) ?? host }

    func matches(_ name: String) -> Bool {
        name.count >= prefix.count + suffix.count && name.hasPrefix(prefix) && name.hasSuffix(suffix)
    }

    func difficulty(_ id: Int64) -> String {
        guard prefix == "WoWCombatLog" else { return "" }
        switch id {
        case 1, 14: return "Normal"
        case 2, 15: return "Heroic"
        case 3, 175: return "10 Normal"
        case 4, 176: return "25 Normal"
        case 5, 193: return "10 Heroic"
        case 6, 194: return "25 Heroic"
        case 7, 17: return "LFR"
        case 8: return "Mythic+"
        case 9, 186: return "40 Player"
        case 16, 23: return "Mythic"
        case 24, 33: return "Timewalking"
        default: return ""
        }
    }

    func startDate(_ url: URL) -> String? {
        guard id == "swtor" else { return nil }
        let stem = url.deletingPathExtension().lastPathComponent.dropFirst(prefix.count)
        let parts = stem.prefix(10).split(separator: "-")
        guard parts.count == 3 else { return nil }
        return "\(parts[1])/\(parts[2])/\(parts[0])"
    }

    func detect() -> URL? {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return dirs
            .map { URL(fileURLWithPath: $0.replacingOccurrences(of: "~", with: home)) }
            .first { (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }
    }

    func logs(in dir: URL) -> [LogFile] {
        let keys: [URLResourceKey] = [.fileSizeKey, .contentModificationDateKey, .isRegularFileKey]
        let items = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: keys)) ?? []

        return items
            .filter { matches($0.lastPathComponent) }
            .compactMap { url in
                guard let v = try? url.resourceValues(forKeys: Set(keys)), v.isRegularFile == true else { return nil }
                return LogFile(url: url, size: Int64(v.fileSize ?? 0), modified: v.contentModificationDate ?? .distantPast)
            }
            .sorted { $0.modified > $1.modified }
    }
}

struct LogFile: Identifiable, Hashable {
    let url: URL
    let size: Int64
    let modified: Date

    var id: URL { url }
}

private func wow(_ id: String, _ name: String, _ host: String, _ flavor: String) -> Game {
    Game(id: id, name: name, host: host, prefix: "WoWCombatLog", suffix: ".txt", latin1: false,
         dirs: ["/Applications/World of Warcraft/\(flavor)/Logs", "~/Applications/World of Warcraft/\(flavor)/Logs"])
}

let games: [Game] = [
    wow("wow", "World of Warcraft", "www.warcraftlogs.com", "_retail_"),
    wow("classic", "WoW Classic", "classic.warcraftlogs.com", "_classic_"),
    wow("fresh", "WoW Anniversary", "fresh.warcraftlogs.com", "_anniversary_"),
    wow("sod", "WoW Season of Discovery", "sod.warcraftlogs.com", "_classic_era_"),
    wow("vanilla", "WoW Classic Era", "vanilla.warcraftlogs.com", "_classic_era_"),
    wow("titan", "WoW Titan Reforged", "titan.warcraftlogs.com", "_classic_titan_"),
    Game(id: "ff", name: "Final Fantasy XIV", host: "www.fflogs.com", prefix: "Network_", suffix: ".log", latin1: false,
         dirs: ["~/Documents/IINACT", "~/Library/Application Support/XIV on Mac/wineprefix/drive_c/users/emet-selch/Documents/IINACT"]),
    Game(id: "eso", name: "Elder Scrolls Online", host: "www.esologs.com", prefix: "Encounter", suffix: ".log", latin1: false,
         dirs: ["~/Documents/Elder Scrolls Online/live/Logs"]),
    Game(id: "swtor", name: "Star Wars: The Old Republic", host: "www.swtorlogs.com", prefix: "combat_", suffix: ".txt", latin1: true,
         dirs: ["~/Documents/Star Wars - The Old Republic/CombatLogs"]),
    Game(id: "fellowship", name: "Fellowship", host: "www.fellowshiplogs.com", prefix: "CombatLog", suffix: ".txt", latin1: false,
         dirs: []),
]

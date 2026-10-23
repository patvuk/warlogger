import AppKit
import SwiftUI

@main
struct WarloggerApp: App {
    @StateObject private var model = Model()

    var body: some Scene {
        MenuBarExtra {
            Panel().environmentObject(model)
        } label: {
            Image(systemName: model.live ? "dot.radiowaves.left.and.right" : model.busy ? "arrow.up.circle" : "scroll")
        }
        .menuBarExtraStyle(.window)
    }
}

final class Cancel: @unchecked Sendable {
    private let lock = NSLock()
    private var flag = false

    var value: Bool {
        get { lock.withLock { flag } }
        set { lock.withLock { flag = newValue } }
    }
}

@MainActor
final class Model: ObservableObject {
    private let defaults = UserDefaults.standard

    @Published var gameId: String { didSet { defaults.set(gameId, forKey: "game"); switchGame() } }
    @Published var email = ""
    @Published var password = ""
    @Published var account: Account?
    @Published var guild = "" { didSet { defaults.set(guild, forKey: "guild") } }
    @Published var region = "" { didSet { defaults.set(region, forKey: "region") } }
    @Published var visibility = "" { didSet { defaults.set(visibility, forKey: "visibility") } }
    @Published var logs: [LogFile] = []
    @Published var selected: Set<URL> = []
    @Published var fights: [Fight]?
    @Published var picked: Set<Int> = []
    @Published var skipTrash: Bool { didSet { defaults.set(skipTrash, forKey: "skip.trash"); pickFights() } }
    @Published var skipWipes: Bool { didSet { defaults.set(skipWipes, forKey: "skip.wipes"); pickFights() } }
    @Published var skipFailed: Bool { didSet { defaults.set(skipFailed, forKey: "skip.failed"); pickFights() } }
    @Published var skipShort: Bool { didSet { defaults.set(skipShort, forKey: "skip.short"); pickFights() } }
    @Published var merge: Bool { didSet { defaults.set(merge, forKey: "merge") } }
    @Published var update: Update?
    @Published var status = ""
    @Published var progress = 0.0
    @Published var report = ""
    @Published var busy = false
    @Published var live = false

    private let cancel = Cancel()
    private var fightsFile: URL?

    var game: Game { games.first { $0.id == gameId } ?? games[0] }
    var folder: URL? { defaults.url(forKey: "dir.\(game.id)") ?? game.detect() }
    var savedPassword: Bool { !email.isEmpty && Keychain.load(email) != nil }

    init() {
        let saved = UserDefaults.standard
        skipTrash = saved.bool(forKey: "skip.trash")
        skipWipes = saved.bool(forKey: "skip.wipes")
        skipFailed = saved.bool(forKey: "skip.failed")
        skipShort = saved.bool(forKey: "skip.short")
        merge = saved.bool(forKey: "merge")
        gameId = saved.string(forKey: "game") ?? "wow"
        Updater.cleanup()
        Task { update = await Updater.check() }
        guild = defaults.string(forKey: "guild") ?? ""
        region = defaults.string(forKey: "region") ?? ""
        visibility = defaults.string(forKey: "visibility") ?? ""
        switchGame()
    }

    func switchGame() {
        email = defaults.string(forKey: "email.\(game.site)") ?? ""
        password = ""
        account = nil
        refresh()
        if savedPassword { signIn() } else { status = "Sign in to \(game.host)" }
    }

    var filter: Filter { Filter(trash: skipTrash, wipes: skipWipes, failed: skipFailed, short: skipShort) }

    func refresh() {
        fights = nil
        logs = folder.map { game.logs(in: $0) } ?? []
        selected = Set(logs.prefix(1).map(\.url))
    }

    func pickFights() {
        guard let fights else { return }
        let filter = self.filter
        picked = Set(fights.indices.filter { filter.keep(game, fights[$0]) })
    }

    func toggleFights() {
        if fights != nil { refresh(); return }
        guard let (email, secret) = credentials() else { status = "Sign in first"; return }
        guard selected.count == 1, let file = selected.first else { status = "Select one log to list its fights"; return }

        let game = self.game
        let cancel = self.cancel
        cancel.value = false
        busy = true
        progress = 0

        let model = self
        Task.detached {
            let notify: (Event) -> Void = { event in Task { @MainActor in model.apply(event) } }
            do {
                let uploader = try await Uploader(game: game, email: email, password: secret, notify: notify)
                let target = await model.target(uploader.account)
                notify(.step("Scanning for fights"))
                let fights = try uploader.scan(file, region: target.region, cancelled: { cancel.value }, notify: notify)
                await model.show(fights, in: file)
            } catch {
                await model.done(.failure(error))
            }
        }
    }

    func show(_ list: [Fight], in file: URL) {
        fightsFile = file
        fights = list
        pickFights()
        status = "\(list.count) fights in \(file.lastPathComponent); pick the ones to upload"
        busy = false
    }

    func splitSelected() {
        let files = logs.filter { selected.contains($0.url) }.map(\.url)
        guard fights == nil, !files.isEmpty else { status = "Select one or more logs to split"; return }

        let game = self.game, short = skipShort, cancel = self.cancel
        cancel.value = false
        busy = true
        progress = 0
        status = "Splitting…"

        let model = self
        Task.detached {
            let notify: (Event) -> Void = { event in Task { @MainActor in model.apply(event) } }
            var text: String
            do {
                let count = try files.reduce(0) { $0 + (try split(game, $1, short: short, cancelled: { cancel.value }, notify: notify)).count }
                text = "Split into \(count) logs"
            } catch {
                text = error.localizedDescription
            }
            await model.splitDone(text)
        }
    }

    func splitDone(_ text: String) {
        busy = false
        refresh()
        status = text
    }

    func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.message = "Choose the \(game.name) log folder"
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }
        defaults.set(url, forKey: "dir.\(game.id)")
        refresh()
    }

    private func credentials() -> (String, String)? {
        let typed = password.isEmpty ? Keychain.load(email) : password
        guard !email.isEmpty, let secret = typed else { return nil }
        return (email, secret)
    }

    private func pick(_ list: [Choice], _ wanted: String) -> String {
        list.first { $0.key == wanted }?.key ?? list.first?.key ?? ""
    }

    func signIn() {
        guard let (email, secret) = credentials() else { status = "Enter your email and password"; return }
        let game = self.game
        busy = true
        status = "Signing in to \(game.host)…"

        Task {
            do {
                let site = Site(game: game)
                await site.prepare()
                let account = try await site.login(email: email, password: secret)
                Keychain.save(email, secret)
                defaults.set(email, forKey: "email.\(game.site)")
                self.account = account
                guild = pick(account.guilds, guild)
                region = pick(account.regions, region)
                visibility = pick(account.visibilities, visibility)
                password = ""
                status = "Signed in as \(account.name)"
            } catch {
                status = error.localizedDescription
            }
            busy = false
        }
    }

    private func target(_ account: Account) -> Target {
        let value = { (list: [Choice], key: String) in list.first { $0.key == key }?.value }
        let guildValue = value(account.guilds, guild)
        return Target(region: value(account.regions, region) ?? account.regions.first?.value,
                      guild: guildValue.flatMap { "\($0)" == "-1" ? nil : $0 },
                      visibility: value(account.visibilities, visibility) ?? account.visibilities.first?.value)
    }

    func start(live: Bool) {
        if self.live { cancel.value = true; status = "Stopping; uploading the last fight…"; return }
        guard let (email, secret) = credentials() else { status = "Sign in first"; return }
        var filter = self.filter
        var files = logs.filter { selected.contains($0.url) }.map(\.url)
        if let fights, let file = fightsFile {
            filter.fights = picked.sorted().map { (fights[$0].start, fights[$0].end) }
            files = picked.isEmpty ? [] : [file]
        }
        let groups = merge || fights != nil ? [files] : files.map { [$0] }
        guard live || !files.isEmpty else { status = "Select logs to upload"; return }
        guard let dir = folder else { status = "Choose the log folder first"; return }

        let game = self.game
        let cancel = self.cancel
        cancel.value = false
        busy = !live
        self.live = live
        progress = 0

        let model = self
        Task.detached {
            let notify: (Event) -> Void = { event in
                Task { @MainActor in model.apply(event) }
            }

            let result: Result<String, Error>
            do {
                let uploader = try await Uploader(game: game, email: email, password: secret, notify: notify)
                let target = await model.target(uploader.account)
                var url = ""
                if live {
                    url = try await uploader.follow(dir, target: target, all: false, cancelled: { cancel.value }, notify: notify)
                } else {
                    for group in groups {
                        url = try await uploader.upload(group, target: target, filter: filter, cancelled: { cancel.value }, notify: notify)
                    }
                }
                result = .success(url)
            } catch {
                result = .failure(error)
            }

            await model.done(result)
        }
    }

    func apply(_ event: Event) {
        switch event {
        case .step(let text): status = text
        case .progress(let p): progress = p
        case .report(let url): report = url
        }
    }

    func done(_ result: Result<String, Error>) {
        switch result {
        case .success(let url): status = "Done: \(url)"
        case .failure(let error): status = error.localizedDescription
        }
        busy = false
        live = false
    }

    func checkUpdate() {
        status = "Checking for updates…"
        Task {
            update = await Updater.check()
            if update == nil { status = "Warlogger \(Updater.current) is up to date" }
        }
    }

    func installUpdate() {
        guard let update else { return }
        busy = true
        status = "Downloading Warlogger \(update.version)…"
        Task {
            do {
                try await Updater.install(update)
            } catch {
                status = "Update failed: \(error.localizedDescription)"
                busy = false
            }
        }
    }

    func openReport() {
        if let url = URL(string: report) { NSWorkspace.shared.open(url) }
    }
}

struct FightRow: Identifiable {
    let id: Int
    let fight: Fight
    let cells: [String]
}

struct Panel: View {
    @EnvironmentObject var model: Model

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let update = model.update {
                HStack {
                    Text("Warlogger \(update.version) is available").font(.callout)
                    Spacer()
                    Button("Update and restart", action: model.installUpdate)
                        .disabled(model.busy || model.live)
                }
            }

            Picker("Game", selection: $model.gameId) {
                ForEach(games) { Text($0.name).tag($0.id) }
            }
            .disabled(model.busy || model.live)

            HStack {
                TextField("Email", text: $model.email)
                SecureField(model.savedPassword ? "Saved password" : "Password", text: $model.password)
                Button("Sign in", action: model.signIn)
            }
            .disabled(model.busy || model.live)

            if let account = model.account {
                HStack {
                    choice("Guild", account.guilds, $model.guild)
                    choice("Region", account.regions, $model.region)
                    choice("Visibility", account.visibilities, $model.visibility)
                }
                .labelsHidden()
                .disabled(model.busy || model.live)
            }

            Divider()

            HStack {
                Text(model.folder?.path ?? "No log folder").font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                Spacer()
                Button("Folder…", action: model.chooseFolder)
                Button(action: model.refresh) { Image(systemName: "arrow.clockwise") }
            }

            if let fights = model.fights {
                let rows = fights.indices.map { FightRow(id: $0, fight: fights[$0], cells: fights[$0].columns(model.game)) }
                Table(rows, selection: $model.picked) {
                    TableColumn("Fight") { cell($0, 0) }.width(min: 120, ideal: 230)
                    TableColumn("Difficulty") { cell($0, 1) }.width(min: 60, ideal: 80)
                    TableColumn("Result") { cell($0, 2) }.width(min: 50, ideal: 80)
                    TableColumn("Duration") { cell($0, 3) }.width(min: 50, ideal: 60)
                }
                .frame(height: 220)
            } else {
                Table(model.logs, selection: $model.selected) {
                    TableColumn("Log") { Text($0.url.lastPathComponent).lineLimit(1).truncationMode(.middle) }
                        .width(min: 120, ideal: 260)
                    TableColumn("Size") { Text(ByteCountFormatter.string(fromByteCount: $0.size, countStyle: .file)) }
                        .width(min: 50, ideal: 70)
                    TableColumn("Modified") { Text($0.modified, style: .relative) }
                        .width(min: 60, ideal: 110)
                }
                .frame(height: 220)
            }

            HStack {
                Toggle("Skip trash", isOn: $model.skipTrash)
                Toggle("Skip wipes", isOn: $model.skipWipes)
                Toggle("Skip failed keys", isOn: $model.skipFailed)
                Toggle("Skip short", isOn: $model.skipShort)
                Toggle("Merge", isOn: $model.merge)
            }
            .toggleStyle(.checkbox)
            .font(.callout)
            .disabled(model.busy || model.live)

            HStack {
                Button(model.fights == nil ? "Fights" : "Logs", action: model.toggleFights)
                    .disabled(model.busy || model.live)
                Button("Split", action: model.splitSelected)
                    .disabled(model.busy || model.live || model.fights != nil)
                Button("Upload selected") { model.start(live: false) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(model.busy || model.live)
                Button(model.live ? "Stop live" : "Start live") { model.start(live: true) }
                    .disabled(model.busy)
                Spacer()
                Button("Open report", action: model.openReport).disabled(model.report.isEmpty)
            }

            if model.busy { ProgressView(value: model.progress) }
            Text(model.status).font(.caption).foregroundStyle(.secondary).lineLimit(3)

            HStack {
                Button("Check for updates", action: model.checkUpdate).buttonStyle(.borderless)
                    .disabled(model.busy || model.live)
                Spacer()
                Button("Quit") { NSApp.terminate(nil) }.buttonStyle(.borderless)
            }
        }
        .padding(16)
        .frame(width: 520)
    }

    private func cell(_ row: FightRow, _ column: Int) -> some View {
        Text(row.cells[column]).fontWeight(row.fight.dungeon ? .bold : .regular).lineLimit(1)
    }

    private func choice(_ title: String, _ list: [Choice], _ binding: Binding<String>) -> some View {
        Picker(title, selection: binding) {
            ForEach(list, id: \.key) { Text($0.label).tag($0.key) }
        }
    }
}

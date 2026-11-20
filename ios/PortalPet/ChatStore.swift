import Combine
import Foundation

@MainActor
final class ChatStore: ObservableObject {
    enum Connection {
        case paused, connecting, online

        var label: String {
            switch self {
            case .paused: return "离线"
            case .connecting: return "连接中"
            case .online: return "已连接"
            }
        }
    }

    @Published private(set) var session: ChatSession?
    @Published private(set) var profileMember: Member?
    @Published private(set) var isAdmin = false
    @Published private(set) var people: [Person] = []
    @Published private(set) var messages: [Int64: [ChatMessage]] = [:]
    @Published private(set) var unread: [Int64: Int] = [:]
    @Published private(set) var connection: Connection = .paused
    @Published private(set) var arrival: ChatMessage?
    @Published private(set) var lastEnding: ConversationEnd?
    @Published private(set) var activating = false
    @Published private(set) var signingOut = false
    @Published private(set) var pendingFriends: Set<Int64> = []
    @Published var loginError: String?
    @Published var notice: String?
    @Published var quiet: Bool {
        didSet {
            preferences.set(quiet, forKey: "portal.quiet")
            if quiet { dismissArrival() }
        }
    }

    var unreadCount: Int { unread.values.reduce(0, +) }
    var pendingRequestCount: Int { people.filter { $0.relationship == .incoming }.count }

    private let api: PortalAPI
    private let preferences: UserDefaults
    private var active = false
    private var visiblePeer: Int64?
    private var seenThrough: [Int64: Int64] = [:]
    private var endedThrough: [Int64: Int64] = [:]
    private var connectionId: UUID?
    private var connectionTask: Task<Void, Never>?
    private var socket: URLSessionWebSocketTask?
    private var syncTask: Task<Void, Never>?
    private var dismissTask: Task<Void, Never>?
    private var directoryRevision = 0

    init(api: PortalAPI = PortalAPI(), preferences: UserDefaults = .standard) {
        self.api = api
        self.preferences = preferences
        self.quiet = preferences.bool(forKey: "portal.quiet")
        if let restored = SessionKeychain.load(),
           restored.server == (try? PortalAPI.origin(PortalAPI.defaultServerAddress)) {
            session = restored
            profileMember = restored.member
            loadReadProgress(restored)
        }
    }

    func setActive(_ active: Bool) {
        self.active = active
        if active {
            connectIfNeeded()
        } else {
            stopConnection()
            dismissArrival()
        }
    }

    func activate(code: String) async {
        guard !activating, !signingOut, session == nil else { return }
        activating = true
        loginError = nil
        defer { activating = false }

        do {
            let server = try PortalAPI.origin(PortalAPI.defaultServerAddress)
            let activated = try await api.activate(server: server, code: code)
            if !SessionKeychain.save(activated) {
                _ = SessionKeychain.clear()
                notice = "无法保存登录凭据，本次登录保留到退出 App。"
            }
            loadReadProgress(activated)
            session = activated
            profileMember = activated.member
            connectIfNeeded()
        } catch {
            loginError = explanation(error)
        }
    }

    func logout() async {
        guard let current = session, !signingOut else { return }
        signingOut = true
        defer { signingOut = false }

        stopConnection()
        var warning: String?
        do {
            try await api.logout(current)
        } catch {
            if (error as? PortalError)?.isUnauthorized != true {
                warning = "已退出本机，服务器暂时未确认注销。"
            }
        }
        if session == current { endSession(message: warning) }
    }

    func refresh() async {
        guard let current = session else { return }
        await synchronize(current, includeHistory: true, reportFailure: true)
    }

    func changeNickname(_ nickname: String) async -> Bool {
        guard let current = session else { return false }
        do {
            let member = try await api.changeNickname(nickname, current)
            guard session == current else { return false }
            profileMember = member
            _ = try? await fetchDirectory(current)
            return session == current
        } catch {
            report(error, for: current)
            return false
        }
    }

    func createAccount(_ nickname: String) async -> IssuedAccount? {
        guard let current = session, isAdmin else { return nil }
        do {
            let account = try await api.createAccount(nickname, current)
            return session == current ? account : nil
        } catch {
            report(error, for: current)
            return nil
        }
    }

    func changeAvatar(_ avatar: Avatar) async {
        guard let current = session else { return }
        do {
            let member = try await api.changeAvatar(avatar, current)
            guard session == current else { return }
            profileMember = member
            _ = try? await fetchDirectory(current)
        } catch {
            report(error, for: current)
        }
    }

    func addFriend(_ person: Person) async {
        await updateFriend(person.id) { current in
            try await api.requestFriend(person.id, current)
        }
    }

    func answerFriend(_ person: Person, accept: Bool) async {
        await updateFriend(person.id) { current in
            try await api.answerFriend(person.id, accept: accept, current)
        }
    }

    func enter(_ peer: Int64) {
        visiblePeer = peer
        markRead(peer)
        if arrival?.sender.id == peer { dismissArrival() }
    }

    func leave(_ peer: Int64) {
        if visiblePeer == peer { visiblePeer = nil }
    }

    func loadHistory(_ peer: Int64) async {
        guard let current = session else { return }
        do {
            try await fetchHistory(peer, current)
        } catch {
            report(error, for: current)
        }
    }

    func send(_ peer: Int64, body: String, id: UUID) async -> Bool {
        guard let current = session else { return false }
        do {
            let sent = try await api.send(peer, body: body, id: id, current)
            guard session == current else { return false }
            merge([sent], for: peer, session: current)
            return true
        } catch {
            report(error, for: current)
            return false
        }
    }

    func endConversation(_ peer: Int64) async -> Bool {
        guard let current = session else { return false }
        do {
            let ending = try await api.endConversation(peer, current)
            guard session == current else { return false }
            applyEnding(ending, session: current, announce: false)
            return (messages[peer]?.last?.id ?? 0) <= ending.throughMessageId
        } catch {
            report(error, for: current)
            return false
        }
    }

    func dismissArrival() {
        dismissTask?.cancel()
        dismissTask = nil
        arrival = nil
    }

    private func connectIfNeeded() {
        guard active, !signingOut, connectionTask == nil, let current = session else { return }
        let id = UUID()
        connectionId = id
        connection = .connecting
        connectionTask = Task { [weak self] in
            await self?.listen(current, id: id)
        }
    }

    private func listen(_ current: ChatSession, id: UUID) async {
        var retry = 1
        while connectionId == id, !Task.isCancelled {
            let currentSocket = api.events(current)
            socket = currentSocket
            currentSocket.resume()
            do {
                while connectionId == id, !Task.isCancelled {
                    let frame = try await currentSocket.receive()
                    guard connectionId == id, session == current, !Task.isCancelled else { break }
                    guard let event = api.event(from: frame) else { continue }
                    switch event.type {
                    case .ready:
                        connection = .online
                        retry = 1
                        scheduleSync(current)
                    case .directoryChanged:
                        await synchronize(current, includeHistory: false, reportFailure: false)
                    case .resync:
                        scheduleSync(current)
                    case .message:
                        if let message = event.message { receive(message, for: current) }
                    case .conversationEnded:
                        if let ending = event.ending { applyEnding(ending, session: current, announce: true) }
                    case .sessionEnded:
                        endSession(message: "登录凭据已失效，请重新激活。")
                    }
                }
            } catch {
                guard connectionId == id, session == current, !Task.isCancelled else { return }
                connection = .connecting
                await synchronize(current, includeHistory: false, reportFailure: false)
            }
            currentSocket.cancel(with: .goingAway, reason: nil)
            guard connectionId == id, !Task.isCancelled else { return }
            socket = nil
            try? await Task.sleep(for: .seconds(retry))
            retry = min(retry * 2, 8)
        }
    }

    private func stopConnection() {
        connectionId = nil
        connectionTask?.cancel()
        connectionTask = nil
        syncTask?.cancel()
        syncTask = nil
        socket?.cancel(with: .goingAway, reason: nil)
        socket = nil
        connection = .paused
    }

    private func scheduleSync(_ current: ChatSession) {
        syncTask?.cancel()
        syncTask = Task { [weak self] in
            await self?.synchronize(current, includeHistory: true, reportFailure: false)
        }
    }

    private func synchronize(_ current: ChatSession, includeHistory: Bool, reportFailure: Bool) async {
        do {
            let directory = try await fetchDirectory(current)
            guard includeHistory else { return }
            for person in directory.people where person.relationship == .friend {
                try Task.checkCancellation()
                do {
                    try await fetchHistory(person.id, current)
                } catch let failure as PortalError where !failure.isUnauthorized {
                    if reportFailure { report(failure, for: current) }
                }
            }
        } catch {
            if reportFailure || (error as? PortalError)?.isUnauthorized == true {
                report(error, for: current)
            }
        }
    }

    private func fetchDirectory(_ current: ChatSession) async throws -> Directory {
        directoryRevision += 1
        let revision = directoryRevision
        let directory = try await api.directory(current)
        guard session == current else { throw CancellationError() }
        try Task.checkCancellation()
        if revision == directoryRevision {
            people = directory.people
            profileMember = directory.me
            isAdmin = directory.isAdmin == true
            for ending in directory.endings ?? [] { applyEnding(ending, session: current, announce: false) }
        }
        return directory
    }

    private func fetchHistory(_ peer: Int64, _ current: ChatSession) async throws {
        let history = try await api.history(peer, current)
        guard session == current else { throw CancellationError() }
        try Task.checkCancellation()
        merge(history, for: peer, session: current)
    }

    private func updateFriend(_ peer: Int64, using action: (ChatSession) async throws -> Void) async {
        guard let current = session, !pendingFriends.contains(peer) else { return }
        pendingFriends.insert(peer)
        defer { pendingFriends.remove(peer) }
        do {
            try await action(current)
            _ = try await fetchDirectory(current)
        } catch {
            report(error, for: current)
        }
    }

    private func receive(_ message: ChatMessage, for current: ChatSession) {
        guard message.recipientId == current.member.id else { return }
        let peer = message.peer(for: current.member.id)
        let isUnread = message.id > (seenThrough[peer] ?? 0)
        let known = messages[peer]?.contains { $0.id == message.id } ?? false
        merge([message], for: peer, session: current)
        guard isUnread, !known, active, !quiet, visiblePeer != peer,
              message.id > (endedThrough[peer] ?? 0) else { return }

        dismissTask?.cancel()
        arrival = message
        dismissTask = Task { [weak self] in
            do { try await Task.sleep(for: .seconds(8)) } catch { return }
            if self?.arrival?.id == message.id { self?.arrival = nil }
        }
    }

    private func applyEnding(_ ending: ConversationEnd, session current: ChatSession, announce: Bool) {
        let peer = ending.other(for: current.member.id)
        let previous = endedThrough[peer] ?? -1
        guard ending.throughMessageId >= previous else { return }
        endedThrough[peer] = ending.throughMessageId
        if let arrival, arrival.sender.id == peer, arrival.id <= ending.throughMessageId { dismissArrival() }
        if announce, (messages[peer]?.last?.id ?? 0) <= ending.throughMessageId { lastEnding = ending }
    }

    private func merge(_ batch: [ChatMessage], for peer: Int64, session current: ChatSession) {
        var byId = Dictionary(uniqueKeysWithValues: (messages[peer] ?? []).map { ($0.id, $0) })
        for message in batch { byId[message.id] = message }
        messages[peer] = Array(byId.values.sorted { $0.id < $1.id }.suffix(100))
        if visiblePeer == peer && active {
            markRead(peer)
        } else {
            unread[peer] = messages[peer]?.filter {
                $0.recipientId == current.member.id && $0.id > (seenThrough[peer] ?? 0)
            }.count ?? 0
        }
    }

    private func markRead(_ peer: Int64) {
        unread[peer] = 0
        guard let current = session,
              let latest = messages[peer]?.last?.id,
              latest > (seenThrough[peer] ?? 0) else { return }
        seenThrough[peer] = latest
        preferences.set(try? JSONEncoder().encode(seenThrough), forKey: readKey(current))
    }

    private func loadReadProgress(_ current: ChatSession) {
        seenThrough = preferences.data(forKey: readKey(current)).flatMap {
            try? JSONDecoder().decode([Int64: Int64].self, from: $0)
        } ?? [:]
    }

    private func readKey(_ current: ChatSession) -> String {
        "portal.read.\(current.server.absoluteString).\(current.member.id)"
    }

    private func endSession(message: String?) {
        stopConnection()
        dismissArrival()
        let cleared = SessionKeychain.clear()
        session = nil
        profileMember = nil
        isAdmin = false
        people = []
        messages = [:]
        unread = [:]
        pendingFriends = []
        seenThrough = [:]
        endedThrough = [:]
        lastEnding = nil
        visiblePeer = nil
        notice = cleared ? message : "无法清除系统保存的登录凭据。"
    }

    private func report(_ error: Error, for current: ChatSession) {
        guard session == current, !signingOut, !(error is CancellationError), !Task.isCancelled else { return }
        if (error as? PortalError)?.isUnauthorized == true {
            endSession(message: "登录凭据已失效，请重新激活。")
        } else {
            notice = explanation(error)
        }
    }

    private func explanation(_ error: Error) -> String {
        if let network = error as? URLError {
            switch network.code {
            case .notConnectedToInternet, .networkConnectionLost: return "网络已断开，请检查连接后重试。"
            case .timedOut: return "连接超时，请稍后重试。"
            case .secureConnectionFailed, .serverCertificateUntrusted, .serverCertificateHasBadDate,
                 .serverCertificateHasUnknownRoot, .serverCertificateNotYetValid:
                return "无法验证服务器的 HTTPS 证书。"
            default: return "无法连接服务器，请检查地址和端口。"
            }
        }
        return error.localizedDescription
    }
}

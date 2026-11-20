import Foundation
import Security

struct Member: Codable, Hashable, Identifiable {
    let id: Int64
    let nickname: String
    let avatar: Avatar?
}

enum Avatar: String, Codable, CaseIterable {
    case robot = "default"
    case blue
    case purple

    var title: String {
        switch self {
        case .robot: return "默认机器人"
        case .blue: return "蓝色机器人"
        case .purple: return "紫色机器人"
        }
    }

    var artworkName: String? { self == .robot ? nil : "avatar_\(rawValue)" }
}

enum Relationship: String, Decodable {
    case none, outgoing, incoming, friend
}

struct Person: Decodable, Identifiable {
    let member: Member
    let online: Bool
    let relationship: Relationship

    var id: Int64 { member.id }
}

struct Directory: Decodable {
    let me: Member
    let isAdmin: Bool?
    let people: [Person]
    let endings: [ConversationEnd]?
}

struct IssuedAccount: Decodable {
    let member: Member
    let code: String
}

private struct NicknameChange: Encodable {
    let nickname: String
}

struct ConversationEnd: Decodable, Equatable {
    let endedBy: Member
    let peerId: Int64
    let throughMessageId: Int64

    func other(for member: Int64) -> Int64 { endedBy.id == member ? peerId : endedBy.id }
}

struct ChatMessage: Decodable, Identifiable, Equatable {
    let id: Int64
    let sender: Member
    let recipientId: Int64
    let body: String
    let sentAt: Int64

    var date: Date { Date(timeIntervalSince1970: TimeInterval(sentAt)) }

    func peer(for member: Int64) -> Int64 {
        sender.id == member ? recipientId : sender.id
    }
}

struct ChatSession: Codable, Equatable {
    let server: URL
    let member: Member
    let token: String
}

struct PortalEvent: Decodable {
    enum Kind: String, Decodable {
        case ready
        case directoryChanged = "directory_changed"
        case message
        case conversationEnded = "conversation_ended"
        case resync
        case sessionEnded = "session_ended"
    }

    let type: Kind
    let message: ChatMessage?
    let ending: ConversationEnd?
}

enum PortalError: LocalizedError {
    case invalidAddress
    case invalidCode
    case invalidResponse
    case server(Int, String)

    var isUnauthorized: Bool {
        if case .server(401, _) = self { return true }
        return false
    }

    var errorDescription: String? {
        switch self {
        case .invalidAddress: return "请输入完整的 HTTPS 地址，例如 https://服务器IP:8443。"
        case .invalidCode: return "请输入管理员发放的 6 位安全码。"
        case .invalidResponse: return "无法读取服务器响应。"
        case let .server(_, message): return message
        }
    }
}

private final class RejectRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(
        _ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void
    ) {
        completionHandler(nil)
    }
}

struct PortalAPI {
    static let defaultServerAddress = "https://chat.example.com"
    static let securityCodeLength = 6

    private let network: URLSession

    init() {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 12
        configuration.urlCache = nil
        network = URLSession(configuration: configuration, delegate: RejectRedirects(), delegateQueue: nil)
    }

    static func origin(_ input: String) throws -> URL {
        guard var parts = URLComponents(string: input.trimmingCharacters(in: .whitespacesAndNewlines)),
              let host = parts.host, !host.isEmpty,
              parts.user == nil, parts.password == nil, parts.query == nil, parts.fragment == nil,
              parts.percentEncodedPath.isEmpty || parts.percentEncodedPath == "/",
              parts.port.map({ (1...65535).contains($0) }) ?? true else {
            throw PortalError.invalidAddress
        }
        let isHTTPS = parts.scheme?.lowercased() == "https"
        #if DEBUG
        let loopback = ["localhost", "127.0.0.1", "[::1]", "::1"].contains(host.lowercased())
        let permitsHTTP = loopback && parts.scheme?.lowercased() == "http"
        #else
        let permitsHTTP = false
        #endif
        guard isHTTPS || permitsHTTP else { throw PortalError.invalidAddress }
        parts.scheme = isHTTPS ? "https" : "http"
        parts.path = ""
        guard let url = parts.url else { throw PortalError.invalidAddress }
        return url
    }

    func activate(server: URL, code: String) async throws -> ChatSession {
        struct Payload: Encodable { let code: String }
        struct Activation: Decodable { let member: Member; let token: String }

        let code = code.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        guard code.utf8.count == Self.securityCodeLength,
              code.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) }) else {
            throw PortalError.invalidCode
        }
        let data = try await request(
            "activate", at: server, method: "POST", body: encode(Payload(code: code))
        )
        let result: Activation = try decode(data)
        return ChatSession(server: server, member: result.member, token: result.token)
    }

    func directory(_ session: ChatSession) async throws -> Directory {
        try decode(await request("directory", at: session.server, token: session.token))
    }

    func changeNickname(_ nickname: String, _ session: ChatSession) async throws -> Member {
        try decode(await request("me/nickname", at: session.server, method: "POST",
                                 token: session.token, body: encode(NicknameChange(nickname: nickname))))
    }

    func changeAvatar(_ avatar: Avatar, _ session: ChatSession) async throws -> Member {
        struct Payload: Encodable { let avatar: Avatar }
        return try decode(await request("me/avatar", at: session.server, method: "POST",
                                        token: session.token, body: encode(Payload(avatar: avatar))))
    }

    func createAccount(_ nickname: String, _ session: ChatSession) async throws -> IssuedAccount {
        try decode(await request("admin/accounts", at: session.server, method: "POST",
                                 token: session.token, body: encode(NicknameChange(nickname: nickname))))
    }

    func history(_ peer: Int64, _ session: ChatSession) async throws -> [ChatMessage] {
        try decode(await request("messages/\(peer)", at: session.server, token: session.token))
    }

    func requestFriend(_ peer: Int64, _ session: ChatSession) async throws {
        _ = try await request("friends/\(peer)/request", at: session.server, method: "POST", token: session.token)
    }

    func answerFriend(_ peer: Int64, accept: Bool, _ session: ChatSession) async throws {
        struct Payload: Encodable { let accept: Bool }

        _ = try await request(
            "friends/\(peer)/answer", at: session.server, method: "POST", token: session.token,
            body: encode(Payload(accept: accept))
        )
    }

    func send(_ peer: Int64, body: String, id: UUID, _ session: ChatSession) async throws -> ChatMessage {
        struct Payload: Encodable { let clientId: String; let body: String }

        return try decode(await request(
            "messages/\(peer)", at: session.server, method: "POST", token: session.token,
            body: encode(Payload(clientId: id.uuidString, body: body))
        ))
    }

    func logout(_ session: ChatSession) async throws {
        _ = try await request("session/logout", at: session.server, method: "POST", token: session.token)
    }

    func endConversation(_ peer: Int64, _ session: ChatSession) async throws -> ConversationEnd {
        try decode(await request("messages/\(peer)/end", at: session.server, method: "POST", token: session.token))
    }

    func events(_ session: ChatSession) -> URLSessionWebSocketTask {
        var parts = URLComponents(url: endpoint("events", at: session.server), resolvingAgainstBaseURL: false)!
        parts.scheme = parts.scheme == "https" ? "wss" : "ws"
        var request = URLRequest(url: parts.url!)
        request.setValue("Bearer \(session.token)", forHTTPHeaderField: "Authorization")
        let socket = network.webSocketTask(with: request)
        socket.maximumMessageSize = 16 * 1024
        return socket
    }

    func event(from message: URLSessionWebSocketTask.Message) -> PortalEvent? {
        let data: Data
        switch message {
        case let .string(text): data = Data(text.utf8)
        case let .data(binary): data = binary
        @unknown default: return nil
        }
        return try? decode(data)
    }

    private func endpoint(_ path: String, at server: URL) -> URL {
        server.appendingPathComponent("api/v1").appendingPathComponent(path)
    }

    private func request(
        _ path: String, at server: URL, method: String = "GET", token: String? = nil, body: Data? = nil
    ) async throws -> Data {
        var request = URLRequest(url: endpoint(path, at: server))
        request.httpMethod = method
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = body
        }
        let (data, response) = try await network.data(for: request)
        guard let response = response as? HTTPURLResponse else { throw PortalError.invalidResponse }
        guard (200..<300).contains(response.statusCode) else {
            struct Failure: Decodable { let message: String }
            let reason = (try? decode(data) as Failure)?.message ?? "服务器返回 HTTP \(response.statusCode)。"
            throw PortalError.server(response.statusCode, reason)
        }
        return data
    }

    private func encode<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return try encoder.encode(value)
    }

    private func decode<T: Decodable>(_ data: Data) throws -> T {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        do {
            return try decoder.decode(T.self, from: data)
        } catch {
            throw PortalError.invalidResponse
        }
    }
}

enum SessionKeychain {
    private static let item: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "org.portalpet.chat.ios",
        kSecAttrAccount as String: "chat-session"
    ]

    static func load() -> ChatSession? {
        var query = item
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data,
              let saved = try? JSONDecoder().decode(ChatSession.self, from: data),
              (try? PortalAPI.origin(saved.server.absoluteString)) == saved.server else { return nil }
        return saved
    }

    static func save(_ session: ChatSession) -> Bool {
        guard let data = try? JSONEncoder().encode(session) else { return false }
        let values = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        ] as [String: Any]
        let status = SecItemUpdate(item as CFDictionary, values as CFDictionary)
        if status == errSecSuccess { return true }
        guard status == errSecItemNotFound else { return false }
        return SecItemAdd(item.merging(values) { _, value in value } as CFDictionary, nil) == errSecSuccess
    }

    @discardableResult
    static func clear() -> Bool {
        let status = SecItemDelete(item as CFDictionary)
        return status == errSecSuccess || status == errSecItemNotFound
    }
}

import ImageIO
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

@main
struct PortalPetApp: App {
    var body: some Scene {
        WindowGroup { PortalRoot() }
    }
}

private struct PortalRoot: View {
    @Environment(\.scenePhase) private var phase
    @StateObject private var store = ChatStore()

    var body: some View {
        Group {
            if let session = store.session {
                HomeTabs(store: store, session: session)
            } else {
                SignInView(store: store)
            }
        }
        .tint(PortalTheme.accent)
        .onChange(of: phase, initial: true) { _, phase in
            store.setActive(phase == .active)
        }
        .alert("Portal Pet", isPresented: Binding(
            get: { store.notice != nil },
            set: { if !$0 { store.notice = nil } }
        )) {
            Button("好", role: .cancel) { store.notice = nil }
        } message: {
            Text(store.notice ?? "")
        }
    }
}

private struct SignInView: View {
    @ObservedObject var store: ChatStore
    @State private var code = ""
    @FocusState private var codeFocused: Bool

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                PortalHero()
                VStack(alignment: .leading, spacing: 6) {
                    Text("Portal Pet")
                        .font(.system(.largeTitle, design: .rounded).weight(.bold))
                        .foregroundStyle(PortalTheme.text)
                        .accessibilityAddTraits(.isHeader)
                    Text("输入管理员发放的 6 位安全码。")
                        .font(.subheadline)
                        .foregroundStyle(PortalTheme.muted)
                }

                VStack(alignment: .leading, spacing: 16) {
                    codeEntry

                    if let error = store.loginError {
                        Label(error, systemImage: "exclamationmark.circle")
                            .font(.footnote)
                            .foregroundStyle(.red)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    if store.activating {
                        ProgressView("正在验证…")
                            .font(.footnote)
                            .foregroundStyle(PortalTheme.muted)
                    }
                }
                .padding(17)
                .background(PortalTheme.surface, in: RoundedRectangle(cornerRadius: 23))
                .overlay { RoundedRectangle(cornerRadius: 23).strokeBorder(PortalTheme.line) }
            }
            .frame(maxWidth: 480)
            .padding(.horizontal, 22)
            .padding(.vertical, 24)
            .frame(maxWidth: .infinity)
        }
        .background(PortalTheme.background.ignoresSafeArea())
        .scrollDismissesKeyboard(.interactively)
    }

    private var codeEntry: some View {
        VStack(alignment: .leading, spacing: 7) {
            Label("安全码", systemImage: "key.fill")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(PortalTheme.accent)
                .accessibilityHidden(true)
            ZStack {
                HStack(spacing: 7) {
                    ForEach(0..<PortalAPI.securityCodeLength, id: \.self) { index in
                        let active = codeFocused && index == code.utf8.count
                        Text(index < code.utf8.count ? "•" : " ")
                            .font(.title3.weight(.semibold))
                            .foregroundStyle(PortalTheme.text)
                            .frame(maxWidth: .infinity, minHeight: 46)
                            .background(PortalTheme.background, in: RoundedRectangle(cornerRadius: 10))
                            .overlay {
                                RoundedRectangle(cornerRadius: 10).strokeBorder(
                                    active ? PortalTheme.accent : PortalTheme.line,
                                    lineWidth: active ? 2 : 1
                                )
                            }
                    }
                }
                .accessibilityHidden(true)
                SecureField("", text: Binding(get: { code }, set: enterCode))
                    .foregroundStyle(.clear)
                    .tint(.clear)
                    .frame(minHeight: 46)
                    .contentShape(Rectangle())
                    .textInputAutocapitalization(.characters)
                    .autocorrectionDisabled()
                    .focused($codeFocused)
                    .disabled(store.activating)
                    .accessibilityLabel("6 位安全码")
                    .accessibilityHint("大写字母和数字，输满后自动登录")
                    .accessibilityIdentifier("portal.code")
            }
        }
    }

    private func enterCode(_ input: String) {
        guard !store.activating, !store.signingOut else { return }
        let normalized = String(decoding: input.utf8.compactMap { byte -> UInt8? in
            switch byte {
            case 48...57, 65...90: return byte
            case 97...122: return byte - 32
            default: return nil
            }
        }, as: UTF8.self)
        guard normalized.utf8.count <= PortalAPI.securityCodeLength else {
            store.loginError = "请输入 6 位安全码。"
            return
        }
        guard normalized != code else { return }
        store.loginError = nil
        code = normalized
        guard code.utf8.count == PortalAPI.securityCodeLength else { return }
        Task {
            await store.activate(code: normalized)
            if store.session == nil {
                code = ""
                codeFocused = true
            }
        }
    }
}

private struct HomeTabs: View {
    private enum Tab { case messages, directory, settings }

    @ObservedObject var store: ChatStore
    let session: ChatSession
    @State private var tab: Tab = .messages
    @State private var path: [Member] = []
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TabView(selection: $tab) {
            NavigationStack(path: $path) {
                MessagesHome(store: store, open: open, findFriends: { tab = .directory })
                    .navigationDestination(for: Member.self) { member in
                        let peer = store.people.first { $0.id == member.id }?.member ?? member
                        ConversationView(store: store, peer: peer, me: session.member)
                    }
            }
            .tabItem { Label("消息", systemImage: "bubble.left.and.bubble.right") }
            .badge(store.unreadCount)
            .tag(Tab.messages)

            NavigationStack {
                DirectoryView(store: store, open: open)
            }
            .tabItem { Label("成员", systemImage: "person.2") }
            .badge(store.pendingRequestCount)
            .tag(Tab.directory)

            NavigationStack {
                SettingsView(store: store, session: session)
            }
            .tabItem { Label("设置", systemImage: "gearshape") }
            .tag(Tab.settings)
        }
        .overlay(alignment: .top) {
            if let arrival = store.arrival {
                PortalArrival(message: arrival, open: {
                    open(arrival.sender)
                }, dismiss: {
                    store.dismissArrival()
                })
                .id(arrival.id)
                .frame(maxWidth: 460)
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .transition(reduceMotion ? .opacity : .move(edge: .top).combined(with: .opacity))
            }
        }
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.26), value: store.arrival?.id)
        .onChange(of: store.lastEnding) { _, ending in
            if let ending, path.last?.id == ending.other(for: session.member.id) {
                path.removeAll()
            }
        }
    }

    private func open(_ member: Member) {
        store.dismissArrival()
        path = [member]
        tab = .messages
    }
}

private struct ConnectionLabel: View {
    let connection: ChatStore.Connection

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(connection == .online ? PortalTheme.positive : PortalTheme.muted)
                .frame(width: 7, height: 7)
                .accessibilityHidden(true)
            Text(connection.label)
                .font(.caption.weight(.semibold))
                .foregroundStyle(PortalTheme.muted)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(PortalTheme.wash, in: Capsule())
        .accessibilityIdentifier("portal.connection")
    }
}

private struct MessagesHome: View {
    @ObservedObject var store: ChatStore
    let open: (Member) -> Void
    let findFriends: () -> Void

    private var friends: [Person] {
        store.people.filter { $0.relationship == .friend }.sorted {
            let first = store.messages[$0.id]?.last?.id ?? 0
            let second = store.messages[$1.id]?.last?.id ?? 0
            return first == second ? $0.member.nickname < $1.member.nickname : first > second
        }
    }

    var body: some View {
        ScrollView {
            if friends.isEmpty {
                ContentUnavailableView {
                    Label("还没有会话", systemImage: "bubble.left.and.bubble.right")
                } description: {
                    Text("加好友后就可以开始聊天。")
                } actions: {
                    Button("查看在线成员", action: findFriends)
                        .buttonStyle(.borderedProminent)
                }
                .padding(.top, 54)
            } else {
                LazyVStack(spacing: 0) {
                    ForEach(friends) { person in
                        let latest = store.messages[person.id]?.last
                        Button { open(person.member) } label: {
                            HStack(spacing: 13) {
                                PetAvatar(member: person.id, avatar: person.member.avatar)
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(person.member.nickname)
                                        .font(.body.weight(.semibold))
                                        .foregroundStyle(PortalTheme.text)
                                        .lineLimit(1)
                                    Text(latest?.body ?? (person.online ? "在线" : "离线"))
                                        .font(.subheadline)
                                        .foregroundStyle(PortalTheme.muted)
                                        .lineLimit(1)
                                }
                                Spacer(minLength: 8)
                                VStack(alignment: .trailing, spacing: 6) {
                                    if let latest {
                                        Text(latest.date, style: .time)
                                            .font(.caption2)
                                            .foregroundStyle(PortalTheme.muted)
                                    }
                                    if let count = store.unread[person.id], count > 0 {
                                        Text(count > 99 ? "99+" : "\(count)")
                                            .font(.caption.weight(.bold))
                                            .foregroundStyle(.white)
                                            .padding(.horizontal, 7)
                                            .padding(.vertical, 3)
                                            .background(PortalTheme.violet, in: Capsule())
                                            .accessibilityLabel("\(count) 条未读")
                                    }
                                }
                            }
                            .padding(.vertical, 12)
                            .padding(.horizontal, 14)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("portal.thread.\(person.id)")
                        if person.id != friends.last?.id {
                            Divider().padding(.leading, 75)
                        }
                    }
                }
                .background(PortalTheme.surface, in: RoundedRectangle(cornerRadius: 22))
                .overlay { RoundedRectangle(cornerRadius: 22).strokeBorder(PortalTheme.line) }
                .padding(16)
            }
        }
        .frame(maxWidth: .infinity)
        .background(PortalTheme.background.ignoresSafeArea())
        .navigationTitle("消息")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { ConnectionLabel(connection: store.connection) }
        }
        .refreshable { await store.refresh() }
    }
}

private struct DirectoryView: View {
    @ObservedObject var store: ChatStore
    let open: (Member) -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 21) {
                section("待处理申请", people: store.people.filter { $0.relationship == .incoming })
                section("好友", people: store.people.filter { $0.relationship == .friend })
                section("其他成员", people: store.people.filter {
                    $0.relationship == .none || $0.relationship == .outgoing
                })
                if store.people.isEmpty {
                    ContentUnavailableView(
                        "暂时没有其他成员", systemImage: "person.2",
                        description: Text("其他成员登录同一台服务器后会出现在这里。")
                    )
                    .padding(.top, 44)
                }
            }
            .padding(16)
        }
        .background(PortalTheme.background.ignoresSafeArea())
        .navigationTitle("成员")
        .refreshable { await store.refresh() }
    }

    @ViewBuilder
    private func section(_ title: String, people: [Person]) -> some View {
        if !people.isEmpty {
            VStack(alignment: .leading, spacing: 9) {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PortalTheme.muted)
                    .padding(.leading, 6)
                    .accessibilityAddTraits(.isHeader)
                VStack(spacing: 0) {
                    ForEach(people) { person in
                        personRow(person)
                        if person.id != people.last?.id { Divider().padding(.leading, 67) }
                    }
                }
                .background(PortalTheme.surface, in: RoundedRectangle(cornerRadius: 20))
                .overlay { RoundedRectangle(cornerRadius: 20).strokeBorder(PortalTheme.line) }
            }
        }
    }

    private func personRow(_ person: Person) -> some View {
        HStack(spacing: 10) {
            PetAvatar(member: person.id, avatar: person.member.avatar, size: 42)
            VStack(alignment: .leading, spacing: 4) {
                Text(person.member.nickname)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PortalTheme.text)
                Text(person.online ? "在线" : "离线")
                    .font(.caption)
                    .foregroundStyle(person.online ? PortalTheme.positive : PortalTheme.muted)
            }
            Spacer(minLength: 2)
            Group {
                if store.pendingFriends.contains(person.id) {
                    ProgressView().padding(.horizontal, 8)
                } else {
                    switch person.relationship {
                    case .none:
                        Button("加好友") { Task { await store.addFriend(person) } }
                            .buttonStyle(.bordered)
                            .accessibilityIdentifier("portal.add.\(person.id)")
                    case .outgoing:
                        Text("等待同意")
                            .foregroundStyle(PortalTheme.muted)
                    case .incoming:
                        Button("拒绝") { Task { await store.answerFriend(person, accept: false) } }
                            .buttonStyle(.borderless)
                            .accessibilityIdentifier("portal.decline.\(person.id)")
                        Button("同意") { Task { await store.answerFriend(person, accept: true) } }
                            .buttonStyle(.borderedProminent)
                            .accessibilityIdentifier("portal.accept.\(person.id)")
                    case .friend:
                        Button("聊天") { open(person.member) }
                            .buttonStyle(.bordered)
                            .accessibilityIdentifier("portal.chat.\(person.id)")
                    }
                }
            }
            .font(.caption.weight(.semibold))
        }
        .padding(.vertical, 9)
        .padding(.horizontal, 12)
    }
}

private struct SettingsView: View {
    @ObservedObject var store: ChatStore
    let session: ChatSession
    @State private var confirming = false
    @State private var nicknameDraft: String?
    @State private var savingNickname = false
    @State private var nicknameSaved = false
    @State private var savingAvatar = false
    @State private var newNickname = ""
    @State private var creatingAccount = false
    @State private var issued: IssuedAccount?

    private var currentNickname: String { store.profileMember?.nickname ?? session.member.nickname }
    private var currentAvatar: Avatar { store.profileMember?.avatar ?? session.member.avatar ?? .robot }
    private var nickname: Binding<String> {
        Binding(get: { nicknameDraft ?? currentNickname }, set: {
            nicknameDraft = $0
            nicknameSaved = false
        })
    }

    var body: some View {
        Form {
            Section {
                HStack(spacing: 12) {
                    PetAvatar(member: session.member.id, avatar: currentAvatar, animate: true)
                    Text(currentNickname)
                        .font(.headline)
                }
                .padding(.vertical, 5)
                LabeledContent("连接状态", value: store.connection.label)
            }
            Section("我的昵称") {
                TextField("输入昵称", text: nickname)
                    .accessibilityIdentifier("portal.nickname")
                    .disabled(savingNickname)
                Button(savingNickname ? "正在保存…" : "保存昵称") {
                    let name = nickname.wrappedValue
                    savingNickname = true
                    nicknameSaved = false
                    Task {
                        if await store.changeNickname(name) {
                            nicknameDraft = nil
                            nicknameSaved = true
                        }
                        savingNickname = false
                    }
                }
                .accessibilityIdentifier("portal.nickname.save")
                .disabled(savingNickname || nickname.wrappedValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                if nicknameSaved { Text("昵称已保存").foregroundStyle(PortalTheme.muted) }
            }
            Section {
                Picker("我的形象", selection: Binding(get: { currentAvatar }, set: { avatar in
                    savingAvatar = true
                    Task {
                        await store.changeAvatar(avatar)
                        savingAvatar = false
                    }
                })) {
                    ForEach(Avatar.allCases, id: \.self) { avatar in
                        Text(avatar.title).tag(avatar)
                    }
                }
                .accessibilityIdentifier("portal.avatar")
                .disabled(savingAvatar)
            }
            if store.isAdmin {
                Section("管理员") {
                    TextField("新账号昵称", text: $newNickname)
                        .accessibilityIdentifier("portal.account.nickname")
                        .disabled(creatingAccount)
                    Button(creatingAccount ? "正在创建…" : "创建账号") {
                        let name = newNickname
                        creatingAccount = true
                        Task {
                            if let account = await store.createAccount(name) {
                                issued = account
                                newNickname = ""
                            }
                            creatingAccount = false
                        }
                    }
                    .accessibilityIdentifier("portal.account.create")
                    .disabled(creatingAccount || newNickname.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    if let account = issued {
                        LabeledContent("\(account.member.nickname) 的安全码") {
                            Text(account.code)
                                .font(.system(.body, design: .monospaced))
                                .textSelection(.enabled)
                                .accessibilityIdentifier("portal.account.code")
                        }
                    }
                }
            }
            Section {
                Toggle("免打扰", isOn: $store.quiet)
                    .accessibilityIdentifier("portal.quiet")
            } footer: {
                Text("开启后，来信仅显示未读数。")
            }
            Section {
                Button("退出登录", role: .destructive) { confirming = true }
                    .disabled(store.signingOut)
            }
        }
        .scrollContentBackground(.hidden)
        .background(PortalTheme.background.ignoresSafeArea())
        .navigationTitle("设置")
        .confirmationDialog("退出当前账号？", isPresented: $confirming, titleVisibility: .visible) {
            Button("退出登录", role: .destructive) { Task { await store.logout() } }
            Button("取消", role: .cancel) {}
        } message: {
            Text("退出后可以用同一个安全码再次登录。")
        }
    }
}

private struct ConversationView: View {
    @ObservedObject var store: ChatStore
    let peer: Member
    let me: Member
    @State private var draft = ""
    @State private var sending = false
    @State private var ending = false
    @State private var attempt: Attempt?
    @State private var selectedPhoto: PhotosPickerItem?
    @State private var preparedPhoto: PhotoAttempt?
    @State private var preparingPhoto = false
    @StateObject private var shelf = StickerShelf()
    @State private var choosingEmoji = false
    @State private var droppingPhoto = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.dismiss) private var dismiss

    private struct Attempt {
        let body: String
        let id: UUID
    }

    private struct PhotoAttempt {
        let data: Data
        let id = UUID()
    }

    private var messages: [ChatMessage] { store.messages[peer.id] ?? [] }
    private var trimmed: String { draft.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var canSend: Bool {
        !sending && !ending && !preparingPhoto && (preparedPhoto != nil || (!trimmed.isEmpty && trimmed.unicodeScalars.count <= 2000))
    }

    var body: some View {
        ScrollViewReader { scroll in
            ScrollView {
                if messages.isEmpty {
                    ContentUnavailableView("暂无消息", systemImage: "bubble.left")
                        .padding(.top, 45)
                } else {
                    LazyVStack(spacing: 14) {
                        ForEach(messages) { message in
                            bubble(message).id(message.id)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 16)
                }
            }
            .scrollDismissesKeyboard(.interactively)
            .onChange(of: messages.last?.id, initial: true) { _, last in
                guard let last else { return }
                withAnimation(reduceMotion ? nil : .easeOut(duration: 0.22)) {
                    scroll.scrollTo(last, anchor: .bottom)
                }
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) { composer }
        .overlay {
            if droppingPhoto && !sending && !ending && !preparingPhoto {
                Label("松开添加图片", systemImage: "photo.badge.plus")
                    .font(.headline).padding(20)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
                    .allowsHitTesting(false)
            }
        }
        .onDrop(of: [.image], isTargeted: $droppingPhoto, perform: acceptPhoto)
        .background(PortalTheme.background.ignoresSafeArea())
        .navigationTitle(peer.nickname)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button(ending ? "正在结束…" : "结束聊天") {
                    ending = true
                    Task {
                        if await store.endConversation(peer.id) { dismiss() }
                        ending = false
                    }
                }
                .disabled(sending || ending)
                .accessibilityIdentifier("portal.end")
            }
        }
        .toolbar(.hidden, for: .tabBar)
        .onAppear { store.enter(peer.id) }
        .onDisappear { store.leave(peer.id) }
        .task(id: peer.id) { await store.loadHistory(peer.id) }
        .task(id: store.session?.member.id) { shelf.load(store.session) }
        .task(id: selectedPhoto) {
            if let selectedPhoto { await preparePhoto(selectedPhoto) }
        }
        .sheet(isPresented: $choosingEmoji) {
            StickerPicker(shelf: shelf) { glyph in
                if (draft + glyph).unicodeScalars.count <= 2000 {
                    draft += glyph
                } else {
                    store.notice = "消息最多 2000 个字符。"
                }
            } selectPicture: { image in
                preparedPhoto = PhotoAttempt(data: image)
            } importPicture: { item in
                try await Self.readPhoto(item)
            }
            .presentationDetents([.medium, .large])
        }
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 6) {
            Button {
                choosingEmoji = true
            } label: {
                Image(systemName: "face.smiling").frame(width: 34, height: 46)
            }
            .disabled(sending || ending || preparingPhoto)
            .accessibilityLabel("表情与收藏")
            PhotosPicker(selection: $selectedPhoto, matching: .images) {
                Image(systemName: "photo").frame(width: 34, height: 46)
            }
            .disabled(sending || ending || preparingPhoto)
            .accessibilityLabel("添加图片")
            if let photo = preparedPhoto, let thumbnail = UIImage(data: photo.data) {
                HStack {
                    Image(uiImage: thumbnail).resizable().scaledToFit().frame(width: 46, height: 46)
                    Text("已选图片").font(.subheadline)
                    Spacer()
                    Button("移除") { preparedPhoto = nil }.disabled(sending)
                }
            } else if preparingPhoto {
                ProgressView("正在读取图片…").frame(maxWidth: .infinity, minHeight: 46, alignment: .leading)
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    TextField("输入消息", text: $draft, axis: .vertical)
                        .lineLimit(1...5)
                        .padding(.horizontal, 13)
                        .padding(.vertical, 11)
                        .background(PortalTheme.background, in: RoundedRectangle(cornerRadius: 16))
                        .foregroundStyle(PortalTheme.text)
                        .accessibilityIdentifier("portal.message")
                    if trimmed.unicodeScalars.count > 2000 {
                        Text("消息最多 2000 个字符")
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                }
            }
            Button(action: send) {
                Group {
                    if sending {
                        ProgressView().tint(.white)
                    } else {
                        Image(systemName: "arrow.up")
                            .font(.system(size: 17, weight: .bold))
                    }
                }
                .frame(width: 46, height: 46)
                .foregroundStyle(.white)
                .background(PortalTheme.violet, in: RoundedRectangle(cornerRadius: 15))
            }
            .buttonStyle(.plain)
            .opacity(canSend || sending ? 1 : 0.44)
            .disabled(!canSend)
            .accessibilityLabel("发送")
            .accessibilityIdentifier("portal.send")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(PortalTheme.surface)
        .overlay(alignment: .top) { Divider() }
    }

    private func bubble(_ message: ChatMessage) -> some View {
        let mine = message.sender.id == me.id
        return HStack(alignment: .top, spacing: 8) {
            if mine { Spacer(minLength: 36) }
            if !mine { PetAvatar(member: peer.id, avatar: peer.avatar, size: 34) }
            VStack(alignment: mine ? .trailing : .leading, spacing: 5) {
                if let image = message.image {
                    ConversationPhoto(store: store, message: message, info: image) { data in
                        do {
                            try shelf.save(data)
                            store.notice = "已收藏为表情。"
                        } catch {
                            store.notice = error.localizedDescription
                        }
                    }
                } else {
                    Text(message.body)
                        .font(.body)
                        .padding(.horizontal, 13)
                        .padding(.vertical, 9)
                        .foregroundStyle(mine ? Color.white : PortalTheme.text)
                        .background(
                            mine ? PortalTheme.violet : PortalTheme.surface,
                            in: UnevenRoundedRectangle(
                                topLeadingRadius: mine ? 17 : 6, bottomLeadingRadius: 17,
                                bottomTrailingRadius: 17, topTrailingRadius: mine ? 6 : 17
                            )
                        )
                        .textSelection(.enabled)
                }
                Text(message.date, style: .time)
                    .font(.caption2)
                    .foregroundStyle(PortalTheme.muted)
            }
            if !mine { Spacer(minLength: 36) }
        }
    }

    private func send() {
        guard canSend else { return }
        if let photo = preparedPhoto {
            sending = true
            Task {
                if await store.sendImage(peer.id, data: photo.data, id: photo.id) { preparedPhoto = nil }
                sending = false
            }
            return
        }
        let body = trimmed
        let pending = attempt.flatMap { $0.body == body ? $0 : nil } ?? Attempt(body: body, id: UUID())
        attempt = pending
        sending = true
        Task {
            if await store.send(peer.id, body: body, id: pending.id) {
                if trimmed == body { draft = "" }
                attempt = nil
            }
            sending = false
        }
    }

    private func preparePhoto(_ item: PhotosPickerItem) async {
        preparingPhoto = true
        defer {
            preparingPhoto = false
            selectedPhoto = nil
        }
        do {
            let prepared = try await Self.readPhoto(item)
            preparedPhoto = PhotoAttempt(data: prepared)
        } catch {
            if !Task.isCancelled { store.notice = "无法读取所选图片，请换一张试试。" }
        }
    }

    private static func readPhoto(_ item: PhotosPickerItem) async throws -> Data {
        guard let raw = try await item.loadTransferable(type: Data.self) else { throw PortalError.invalidResponse }
        let image = await Task.detached(priority: .userInitiated) { Self.photoData(raw) }.value
        try Task.checkCancellation()
        guard let image else { throw PortalError.invalidResponse }
        return image
    }

    private func acceptPhoto(_ providers: [NSItemProvider]) -> Bool {
        guard !sending, !ending, !preparingPhoto,
              let provider = providers.first(where: { $0.hasItemConformingToTypeIdentifier(UTType.image.identifier) }),
              let type = provider.registeredTypeIdentifiers.first(where: { UTType($0)?.conforms(to: .image) == true }) else {
            return false
        }
        preparingPhoto = true
        let session = store.session
        provider.loadDataRepresentation(forTypeIdentifier: type) { raw, _ in
            Task { @MainActor in
                defer { preparingPhoto = false }
                let prepared = await Task.detached(priority: .userInitiated) { raw.flatMap(Self.photoData) }.value
                guard store.session == session else { return }
                if let prepared { preparedPhoto = PhotoAttempt(data: prepared) }
                else { store.notice = "无法读取拖入的图片。" }
            }
        }
        return true
    }

    nonisolated private static func photoData(_ raw: Data) -> Data? {
        guard let source = CGImageSourceCreateWithData(raw as CFData, nil),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                  kCGImageSourceCreateThumbnailFromImageAlways: true,
                  kCGImageSourceCreateThumbnailWithTransform: true,
                  kCGImageSourceThumbnailMaxPixelSize: 2400
              ] as CFDictionary) else { return nil }
        let photo = UIImage(cgImage: image)
        if image.alphaInfo == .premultipliedFirst || image.alphaInfo == .premultipliedLast,
           let png = photo.pngData(), png.count <= PortalAPI.maxImageBytes { return png }
        guard let jpeg = photo.jpegData(compressionQuality: 0.86), jpeg.count <= PortalAPI.maxImageBytes else { return nil }
        return jpeg
    }
}

private struct ConversationPhoto: View {
    let store: ChatStore
    let message: ChatMessage
    let info: ChatImage
    let save: (Data) -> Void
    @State private var sourceData: Data?
    @State private var photo: UIImage?
    @State private var failed = false
    @State private var attempt = 0
    @State private var enlarged = false

    var body: some View {
        Group {
            if let photo {
                Button { enlarged = true } label: {
                    Image(uiImage: photo).resizable().scaledToFit()
                }
                .buttonStyle(.plain)
                .accessibilityLabel("查看聊天图片")
                .accessibilityAction(named: "收藏为表情") { if let sourceData { save(sourceData) } }
                .contextMenu {
                    Button("收藏为表情", systemImage: "star") { if let sourceData { save(sourceData) } }
                }
                .sheet(isPresented: $enlarged) {
                    NavigationStack {
                        Image(uiImage: sourceData.flatMap { UIImage(data: $0) } ?? photo).resizable().scaledToFit()
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                            .toolbar { Button("关闭") { enlarged = false } }
                    }
                }
            } else if failed {
                Button("图片未加载，点此重试") {
                    failed = false
                    attempt += 1
                }
                .font(.caption)
            } else {
                ProgressView().accessibilityLabel("正在加载图片")
            }
        }
        .frame(width: 200, height: min(200, max(72, 200 * CGFloat(info.height) / CGFloat(max(info.width, 1)))))
        .background(PortalTheme.surface, in: RoundedRectangle(cornerRadius: 12))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .task(id: attempt) {
            guard photo == nil else { return }
            do {
                let data = try await store.imageData(message)
                let thumbnail = await Task.detached(priority: .userInitiated) {
                    guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil as CGImage? }
                    return CGImageSourceCreateThumbnailAtIndex(source, 0, [
                        kCGImageSourceCreateThumbnailFromImageAlways: true,
                        kCGImageSourceCreateThumbnailWithTransform: true,
                        kCGImageSourceThumbnailMaxPixelSize: 640
                    ] as CFDictionary)
                }.value
                guard !Task.isCancelled else { return }
                photo = thumbnail.map { UIImage(cgImage: $0) }
                sourceData = data
                failed = photo == nil
            } catch {
                if !Task.isCancelled { failed = true }
            }
        }
    }
}

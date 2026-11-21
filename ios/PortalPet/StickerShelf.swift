import CryptoKit
import Foundation
import ImageIO
import PhotosUI
import SwiftUI

@MainActor
final class StickerShelf: ObservableObject {
    struct Emoji: Identifiable {
        var id: String { glyph }
        let glyph: String
        let name: String
    }

    struct Picture: Identifiable {
        var id: String { url.lastPathComponent }
        let url: URL
        let bytes: Int
        let created: Date
    }

    static let groups: [(name: String, items: [Emoji])] = [
        ("笑脸", emojis("😀 开心|😂 笑哭|🤣 笑翻|😊 微笑|😉 眨眼|🥰 喜欢|😍 心动|😘 飞吻|😋 好吃|😜 调皮|🤭 偷笑|🤫 保密|🤔 思考|🙄 白眼|🥺 撒娇|🥹 感动|😢 流泪|😭 大哭|😡 生气|😳 脸红|😅 尴尬|😴 晚安|🥳 庆祝|😎 墨镜|🤩 星星眼|🤗 抱抱|🤖 机器人")),
        ("手势", emojis("👋 你好 再见|👌 好的|👍 点赞|👎 不赞同|✌️ 胜利|🤝 握手|👏 鼓掌|🙌 欢呼|🙏 谢谢|💪 加油|🫶 比心|🤷 摊手|🫡 敬礼")),
        ("爱心", emojis("❤️ 红心 爱你|🧡 橙心|💛 黄心|💚 绿心|💙 蓝心|💜 紫心|🩷 粉心|🤍 白心|💔 心碎|💕 双心|💖 闪亮|💌 情书 信封|🌹 玫瑰|💐 花束")),
        ("动物", emojis("🐶 小狗|🐱 小猫|🐰 兔子|🦊 狐狸|🐻 小熊|🐼 熊猫|🐸 青蛙|🙈 捂眼|🐧 企鹅|🦋 蝴蝶")),
        ("日常", emojis("☀️ 早安 太阳|🌙 晚安 月亮|⭐ 星星|✨ 闪亮|🌈 彩虹|🌱 发芽|🌸 樱花|☕ 咖啡|🧋 奶茶|🎂 生日 蛋糕|🎁 礼物|🎉 庆祝 恭喜|📷 相机|🎵 音乐|🔥 热门|✅ 完成|🚪 传送门"))
    ]

    @Published private(set) var recent: [String] = []
    @Published private(set) var favorites: [String] = []
    @Published private(set) var pictures: [Picture] = []

    private var key: String?
    private var directory: URL?

    func load(_ session: ChatSession?) {
        let scope = session.map {
            Self.digest(Data("\($0.server.absoluteString)#\($0.member.id)".utf8))
        }
        guard key != scope else { return }
        key = scope
        recent = []
        favorites = []
        pictures = []
        directory = nil
        guard let scope else { return }
        let defaults = UserDefaults.standard
        recent = defaults.stringArray(forKey: "portal.stickers.recent.\(scope)") ?? []
        favorites = defaults.stringArray(forKey: "portal.stickers.favorites.\(scope)") ?? []
        do {
            let root = try FileManager.default.url(
                for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            let folder = root.appendingPathComponent("Stickers", isDirectory: true)
                .appendingPathComponent(scope, isDirectory: true)
            directory = folder
            if FileManager.default.fileExists(atPath: folder.path) {
                try refresh(folder)
            }
        } catch {
            directory = nil
        }
    }

    func used(_ glyph: String) {
        guard let key else { return }
        recent = Array(([glyph] + recent.filter { $0 != glyph }).prefix(32))
        UserDefaults.standard.set(recent, forKey: "portal.stickers.recent.\(key)")
    }

    func toggle(_ glyph: String) {
        guard let key else { return }
        if favorites.contains(glyph) {
            favorites.removeAll { $0 == glyph }
        } else {
            favorites = Array(([glyph] + favorites).prefix(80))
        }
        UserDefaults.standard.set(favorites, forKey: "portal.stickers.favorites.\(key)")
    }

    func save(_ data: Data) throws {
        guard let directory else { throw ShelfError.unavailable }
        guard !data.isEmpty, data.count <= PortalAPI.maxImageBytes else { throw ShelfError.invalidSize }
        let file = directory.appendingPathComponent(Self.digest(data))
        guard !FileManager.default.fileExists(atPath: file.path) else { return }
        guard pictures.count < 80, pictures.reduce(data.count, { $0 + $1.bytes }) <= 64 * 1024 * 1024 else {
            throw ShelfError.full
        }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        try refresh(directory)
    }

    func remove(_ picture: Picture) throws {
        guard let directory, pictures.contains(where: { $0.id == picture.id }) else { throw ShelfError.unavailable }
        try FileManager.default.removeItem(at: picture.url)
        try refresh(directory)
    }

    private func refresh(_ directory: URL) throws {
        let keys: [URLResourceKey] = [.isRegularFileKey, .fileSizeKey, .contentModificationDateKey]
        pictures = try FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: keys, options: .skipsHiddenFiles
        ).compactMap { url in
            let values = try url.resourceValues(forKeys: Set(keys))
            guard values.isRegularFile == true, let bytes = values.fileSize,
                  (1...PortalAPI.maxImageBytes).contains(bytes) else { return nil }
            return Picture(url: url, bytes: bytes, created: values.contentModificationDate ?? .distantPast)
        }.sorted { $0.created > $1.created }
    }

    private static func digest(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    private static func emojis(_ words: String) -> [Emoji] {
        words.split(separator: "|").compactMap { item in
            let parts = item.split(separator: " ", maxSplits: 1)
            guard parts.count == 2 else { return nil }
            return Emoji(glyph: String(parts[0]), name: String(parts[1]))
        }
    }

    private enum ShelfError: LocalizedError {
        case unavailable, invalidSize, full

        var errorDescription: String? {
            switch self {
            case .unavailable: "无法访问本机表情收藏"
            case .invalidSize: "每张图片不能超过 8 MiB"
            case .full: "表情收藏已满，请先移除一些图片"
            }
        }
    }
}

struct StickerPicker: View {
    @ObservedObject var shelf: StickerShelf
    var selectEmoji: (String) -> Void
    var selectPicture: (Data) -> Void
    var importPicture: (PhotosPickerItem) async throws -> Data

    @State private var category = "笑脸"
    @State private var search = ""
    @State private var managing = false
    @State private var imported: PhotosPickerItem?
    @State private var failure: String?
    @Environment(\.dismiss) private var dismiss

    private var emoji: [StickerShelf.Emoji] {
        let all = StickerShelf.groups.flatMap(\.items)
        let items: [StickerShelf.Emoji]
        switch category {
        case "最近": items = shelf.recent.compactMap { glyph in all.first { $0.glyph == glyph } }
        case "收藏": items = shelf.favorites.compactMap { glyph in all.first { $0.glyph == glyph } }
        default:
            items = search.trimmingCharacters(in: .whitespaces).isEmpty
                ? StickerShelf.groups.first { $0.name == category }?.items ?? [] : all
        }
        let query = search.trimmingCharacters(in: .whitespaces)
        return query.isEmpty ? items : items.filter { $0.name.localizedCaseInsensitiveContains(query) || $0.glyph.contains(query) }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    TextField("搜索表情", text: $search)
                        .textFieldStyle(.roundedBorder)
                    ScrollView(.horizontal) {
                        HStack {
                            ForEach(["最近", "收藏"] + StickerShelf.groups.map(\.name), id: \.self) { name in
                                Button(name) { category = name; search = "" }
                                    .buttonStyle(.bordered)
                                    .tint(category == name ? PortalTheme.violet : PortalTheme.muted)
                                    .accessibilityAddTraits(category == name ? .isSelected : [])
                            }
                        }
                    }
                    .scrollIndicators(.hidden)
                    if emoji.isEmpty && category != "收藏" {
                        Text(search.isEmpty ? "用过的表情会出现在这里" : "没有找到表情")
                            .font(.caption).foregroundStyle(PortalTheme.muted)
                    }
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 42), spacing: 6)], spacing: 7) {
                        ForEach(emoji) { item in
                            let saved = shelf.favorites.contains(item.glyph)
                            Button {
                                shelf.used(item.glyph)
                                selectEmoji(item.glyph)
                                dismiss()
                            } label: {
                                Text(item.glyph).font(.system(size: 28)).frame(maxWidth: .infinity, minHeight: 44)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel(item.name)
                            .accessibilityAction(named: saved ? "取消收藏" : "收藏") { shelf.toggle(item.glyph) }
                            .contextMenu {
                                Button(saved ? "取消收藏" : "收藏", systemImage: saved ? "star.slash" : "star") {
                                    shelf.toggle(item.glyph)
                                }
                            }
                        }
                    }
                    if category == "收藏" && search.trimmingCharacters(in: .whitespaces).isEmpty {
                        pictures
                    }
                    if let failure {
                        Text(failure).font(.caption).foregroundStyle(.red).accessibilityAddTraits(.updatesFrequently)
                    }
                }
                .padding(16)
            }
            .navigationTitle("表情")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { Button("关闭") { dismiss() } }
        }
        .onAppear {
            if !shelf.recent.isEmpty { category = "最近" }
        }
        .task(id: imported) {
            guard let imported else { return }
            defer { self.imported = nil }
            do {
                let data = try await importPicture(imported)
                try Task.checkCancellation()
                try shelf.save(data)
                failure = nil
            } catch {
                if !Task.isCancelled { failure = error.localizedDescription }
            }
        }
    }

    private var pictures: some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack {
                Text("图片表情").font(.headline)
                Spacer()
                PhotosPicker(selection: $imported, matching: .images) {
                    Label("导入", systemImage: "photo.badge.plus")
                }.disabled(imported != nil)
                Button(managing ? "完成" : "管理") { managing.toggle() }
                    .disabled(shelf.pictures.isEmpty)
            }
            if shelf.pictures.isEmpty {
                Text("长按聊天图片即可收藏").font(.caption).foregroundStyle(PortalTheme.muted)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 54), spacing: 7)], spacing: 8) {
                ForEach(shelf.pictures) { picture in
                    SavedPictureTile(picture: picture, managing: managing) {
                        do {
                            selectPicture(try Data(contentsOf: picture.url))
                            dismiss()
                        } catch { failure = error.localizedDescription }
                    } remove: {
                        do { try shelf.remove(picture) }
                        catch { failure = error.localizedDescription }
                    }
                }
            }
        }
    }
}

private struct SavedPictureTile: View {
    let picture: StickerShelf.Picture
    let managing: Bool
    let choose: () -> Void
    let remove: () -> Void
    @State private var thumbnail: UIImage?

    var body: some View {
        Button(action: managing ? remove : choose) {
            ZStack(alignment: .topTrailing) {
                Group {
                    if let thumbnail {
                        Image(uiImage: thumbnail).resizable().scaledToFit()
                    } else {
                        Image(systemName: "photo")
                    }
                }
                .frame(width: 52, height: 52)
                .background(PortalTheme.background, in: RoundedRectangle(cornerRadius: 8))
                if managing {
                    Image(systemName: "minus.circle.fill")
                        .foregroundStyle(.white, PortalTheme.violet)
                }
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(managing ? "移除图片表情" : "使用图片表情")
        .task(id: picture.id) {
            let url = picture.url
            let image = await Task.detached(priority: .utility) {
                guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil as CGImage? }
                return CGImageSourceCreateThumbnailAtIndex(source, 0, [
                    kCGImageSourceCreateThumbnailFromImageAlways: true,
                    kCGImageSourceCreateThumbnailWithTransform: true,
                    kCGImageSourceThumbnailMaxPixelSize: 120
                ] as CFDictionary)
            }.value
            if !Task.isCancelled { thumbnail = image.map { UIImage(cgImage: $0) } }
        }
    }
}

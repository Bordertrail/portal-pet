import SwiftUI

enum PortalTheme {
    static let background = adaptive(0xF8F6FC, 0x171327)
    static let surface = adaptive(0xFFFFFF, 0x242037)
    static let wash = adaptive(0xEFEBFC, 0x322A51)
    static let line = adaptive(0xE9E3F4, 0x413754)
    static let text = adaptive(0x241B37, 0xF7F1FF)
    static let muted = adaptive(0x625970, 0xC6BED6)
    static let accent = adaptive(0x6042BD, 0xC2ADFF)
    static let positive = adaptive(0x27794F, 0x83DEA9)
    static let violet = Color(rgb: 0x6241BC)

    private static func adaptive(_ light: UInt32, _ dark: UInt32) -> Color {
        Color(uiColor: UIColor { traits in
            UIColor(rgb: traits.userInterfaceStyle == .dark ? dark : light)
        })
    }
}

private extension UIColor {
    convenience init(rgb: UInt32) {
        self.init(
            red: CGFloat((rgb >> 16) & 255) / 255,
            green: CGFloat((rgb >> 8) & 255) / 255,
            blue: CGFloat(rgb & 255) / 255,
            alpha: 1
        )
    }
}

extension Color {
    init(rgb: UInt32) { self.init(uiColor: UIColor(rgb: rgb)) }
}

struct PixelPet: View {
    var member: Int64? = nil
    var avatar: Avatar? = nil
    var idle = false
    var walkStarted: Date? = nil

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private static let sprite = [
        "..XX............", ".XaaX...........", "..XX...XXXXX....", "...XXXXhhhXX....",
        "...XhhhhhhhXX...", "..XXggggggggXX..", "..XbbbbbbbbbbX..", "..XbsssssbsbbX..",
        "..XbbbbbbbbbXX..", "...XXhhhhhXXX...", "...XgaaaaagX....", "...XgafafagX....",
        "...XgaafaagX....", "....XXXXXXXX....", "...XggX..XggX...", "...XXXX..XXXX...",
    ].map(Array.init)

    var body: some View {
        Group {
            if let artwork = avatar?.artworkName {
                if let start = walkStarted, !reduceMotion {
                    WalkingPixelPet(artwork: artwork, legLine: 0.82, start: start)
                } else if idle && !reduceMotion {
                    TimelineView(.animation(minimumInterval: 1.0 / 20.0)) { context in
                        let phase = context.date.timeIntervalSinceReferenceDate * (2 * Double.pi / 3.2)
                        let breath = (1 - cos(phase)) / 2
                        picture(artwork)
                            .scaleEffect(x: 1 + 0.014 * breath, y: 1 - 0.035 * breath, anchor: .bottom)
                    }
                } else {
                    picture(artwork)
                }
            } else {
                robot
            }
        }
        .aspectRatio(1, contentMode: .fit)
        .accessibilityHidden(true)
    }

    private func picture(_ artwork: String) -> some View {
        Image(artwork).resizable().interpolation(.none).scaledToFit()
    }

    private var robot: some View {
        Canvas { context, size in
            let unit = floor(min(size.width, size.height) / 16 * 3) / 3
            let xOffset = (size.width - unit * 16) / 2
            let yOffset = (size.height - unit * 16) / 2
            let palette = member.map { Int($0.magnitude % 3) + 1 } ?? 0
            let bodies: [UInt32] = [0xBE5D43, 0xBD526D, 0x4389A7, 0x8559B7]
            let panels: [UInt32] = [0xF2A879, 0xF596A8, 0x89CADA, 0xC399E6]
            let colors: [Character: UInt32] = [
                "X": 0x241D33, "b": 0x3A2C53, "s": 0xA6F3D6, "f": 0x705014, "a": 0xF9D16F,
                "g": bodies[palette], "h": panels[palette]
            ]
            for (y, row) in Self.sprite.enumerated() {
                for (x, pixel) in row.enumerated() {
                    guard let color = colors[pixel] else { continue }
                    let rect = CGRect(
                        x: xOffset + CGFloat(x) * unit, y: yOffset + CGFloat(y) * unit,
                        width: unit, height: unit
                    )
                    context.fill(Path(rect), with: .color(Color(rgb: color)))
                }
            }
        }
    }
}

private struct WalkingPixelPet: View {
    let artwork: String
    let legLine: CGFloat
    let start: Date

    var body: some View {
        TimelineView(.periodic(from: start, by: 0.055)) { timeline in
            Canvas { context, size in
                let side = min(size.width, size.height)
                let frame = CGRect(x: (size.width - side) / 2, y: (size.height - side) / 2,
                                   width: side, height: side)
                let image = context.resolve(Image(artwork).interpolation(.none))
                let elapsed = max(0, timeline.date.timeIntervalSince(start))
                let step = Int(elapsed / 0.055) % 4
                let joint = frame.minY + side * (legLine - 0.02)
                var torso = context
                torso.clip(to: Path(CGRect(x: frame.minX, y: frame.minY,
                                           width: side, height: side * legLine)))
                torso.draw(image, in: frame)
                for index in 0..<2 {
                    let moving = index == 0 ? step == 1 : step == 3
                    var leg = context
                    if moving { leg.translateBy(x: index == 0 ? -1 : 1, y: -2) }
                    leg.clip(to: Path(CGRect(x: frame.minX + CGFloat(index) * side / 2, y: joint,
                                            width: side / 2, height: frame.maxY - joint)))
                    leg.draw(image, in: frame)
                }
            }
        }
    }
}

struct PetAvatar: View {
    let member: Int64?
    var avatar: Avatar? = nil
    var size: CGFloat = 48
    var animate = false

    var body: some View {
        PixelPet(member: member, avatar: avatar, idle: animate)
            .padding(size * 0.16)
            .frame(width: size, height: size)
            .background(PortalTheme.wash, in: RoundedRectangle(cornerRadius: size * 0.28))
    }
}

private struct PortalGate: View {
    var body: some View {
        GeometryReader { geometry in
            let radius = geometry.size.width * 0.52
            let arch = UnevenRoundedRectangle(
                topLeadingRadius: radius, bottomLeadingRadius: 5,
                bottomTrailingRadius: 5, topTrailingRadius: radius
            )
            arch
                .fill(
                    LinearGradient(
                        colors: [Color(rgb: 0xF3DFFF), Color(rgb: 0xA17DF5), Color(rgb: 0x6844BC)],
                        startPoint: .topLeading, endPoint: .bottomTrailing
                    )
                )
                .overlay {
                    arch
                        .inset(by: max(4, geometry.size.width * 0.12))
                        .fill(
                            RadialGradient(
                                colors: [Color(rgb: 0xEAD4FF), Color(rgb: 0xA87BEA), Color(rgb: 0x422489)],
                                center: .init(x: 0.42, y: 0.42),
                                startRadius: 0, endRadius: geometry.size.height * 0.7
                            )
                        )
                }
                .overlay { arch.strokeBorder(Color(rgb: 0xC8A9FA), lineWidth: 1.5) }
                .shadow(color: Color(rgb: 0x9E75F9).opacity(0.55), radius: 13, y: 3)
        }
        .accessibilityHidden(true)
    }
}

struct PortalHero: View {
    var body: some View {
        GeometryReader { proxy in
            ZStack {
                RoundedRectangle(cornerRadius: 27)
                    .fill(
                        LinearGradient(
                            colors: [Color(rgb: 0x2B1A53), Color(rgb: 0x53308E), Color(rgb: 0x2B1D5B)],
                            startPoint: .topLeading, endPoint: .bottomTrailing
                        )
                    )
                Canvas { context, size in
                    for x in stride(from: 12.0, to: size.width, by: 17) {
                        for y in stride(from: 12.0, to: size.height, by: 17) {
                            let dot = CGRect(x: x, y: y, width: 1.3, height: 1.3)
                            context.fill(Path(dot), with: .color(Color.white.opacity(0.15)))
                        }
                    }
                }
                Ellipse()
                    .fill(Color(rgb: 0xB592FF).opacity(0.20))
                    .frame(width: min(236, proxy.size.width - 20), height: 24)
                    .offset(y: 55)
                PortalGate()
                    .frame(width: 72, height: 103)
                    .offset(x: -41, y: -1)
                PixelPet()
                    .frame(width: 68, height: 68)
                    .shadow(color: .black.opacity(0.3), radius: 5, y: 6)
                    .offset(x: 35, y: 21)
                Image(systemName: "sparkle")
                    .font(.system(size: 22, weight: .medium))
                    .foregroundStyle(Color(rgb: 0xEDDAFF))
                    .offset(x: 83, y: -34)
                Image(systemName: "sparkle")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Color(rgb: 0xDBC2FF))
                    .offset(x: -83, y: 13)
            }
            .clipShape(RoundedRectangle(cornerRadius: 27))
        }
        .frame(height: 158)
        .accessibilityHidden(true)
    }
}

struct PortalArrival: View {
    let message: ChatMessage
    let open: () -> Void
    let dismiss: () -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var emerged = false
    @State private var walkStarted: Date?

    var body: some View {
        HStack(spacing: 4) {
            Button(action: open) {
                HStack(spacing: 4) {
                    ZStack {
                        PortalGate()
                            .frame(width: 35, height: 50)
                            .offset(x: -19, y: -1)
                        PixelPet(member: message.sender.id, avatar: message.sender.avatar, walkStarted: walkStarted)
                            .frame(width: 44, height: 44)
                            .offset(x: emerged ? 15 : -17, y: 6)
                            .scaleEffect(emerged ? 1 : 0.61)
                            .opacity(emerged ? 1 : 0)
                    }
                    .frame(width: 81, height: 62)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(message.sender.nickname)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(PortalTheme.accent)
                            .lineLimit(1)
                        Text(message.body)
                            .font(.subheadline)
                            .foregroundStyle(PortalTheme.text)
                            .lineLimit(2)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(message.sender.nickname) 的消息：\(message.body)")
            .accessibilityHint("打开聊天")
            .accessibilityIdentifier("portal.arrival.open")
            Button(action: dismiss) {
                Image(systemName: "xmark")
                    .font(.system(size: 12, weight: .semibold))
                    .frame(width: 44, height: 44)
            }
            .buttonStyle(.plain)
            .foregroundStyle(PortalTheme.muted)
            .accessibilityLabel("收起传送门")
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 4)
        .background(PortalTheme.surface, in: RoundedRectangle(cornerRadius: 23))
        .overlay { RoundedRectangle(cornerRadius: 23).strokeBorder(PortalTheme.line) }
        .shadow(color: Color(rgb: 0x21143E).opacity(0.16), radius: 18, y: 7)
        .task(id: message.id) {
            emerged = reduceMotion
            walkStarted = nil
            guard !reduceMotion else { return }
            do { try await Task.sleep(for: .milliseconds(110)) } catch { return }
            walkStarted = Date()
            withAnimation(.spring(response: 0.48, dampingFraction: 0.72)) { emerged = true }
            do { try await Task.sleep(for: .milliseconds(550)) } catch { return }
            walkStarted = nil
        }
    }
}

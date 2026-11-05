import SwiftUI
import UIKit

/// How a test tells this screen from a system grouped list.
enum ProductSurface {
    static let identifier = "satchel-product-surface"
    static let container = "scroll-stack"
    static let mark = "satchel-mark"
}

/// The palette in docs/identidade-visual.md. Every color follows the system's light or dark mode.
enum SatchelInk {
    static let canvas = adaptive(0xF3EEE4, 0x17130F)
    static let card = adaptive(0xFFFCF7, 0x221C17)
    static let ink = adaptive(0x241C16, 0xF2ECE3)
    static let secondary = adaptive(0x6B5E52, 0xB8AB9C)
    static let accent = adaptive(0x8C4A24, 0xE0A070)
    /// Text on an `accent` fill.
    static let onAccent = card
    static let line = adaptive(0x241C16, 0xF2ECE3, alpha: 0.14)
    static let field = adaptive(0xEBE4D8, 0x2D261F)
    static let danger = adaptive(0xA3261B, 0xF08A7E)
    static let selected = adaptive(0x8C4A24, 0xE0A070, alpha: 0.14, darkAlpha: 0.18)

    private static func adaptive(
        _ light: UInt32,
        _ dark: UInt32,
        alpha: CGFloat = 1,
        darkAlpha: CGFloat? = nil
    ) -> Color {
        Color(UIColor { traits in
            traits.userInterfaceStyle == .dark
                ? rgb(dark, alpha: darkAlpha ?? alpha)
                : rgb(light, alpha: alpha)
        })
    }

    private static func rgb(_ value: UInt32, alpha: CGFloat) -> UIColor {
        UIColor(
            red: CGFloat((value >> 16) & 0xFF) / 255,
            green: CGFloat((value >> 8) & 0xFF) / 255,
            blue: CGFloat(value & 0xFF) / 255,
            alpha: alpha
        )
    }
}

/// Custom screen container. A vertical stack and a scroll view, never a grouped `List`.
struct ProductScreen<Content: View>: View {
    let title: String
    var showsMark: Bool = true
    var backLabel: String?
    var backDisabled: Bool = false
    var onBack: (() -> Void)?
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 14) {
                if showsMark {
                    HStack(spacing: 12) {
                        Image("SatchelMark")
                            .resizable()
                            .interpolation(.high)
                            .scaledToFit()
                            .frame(width: 36, height: 36)
                            .accessibilityIdentifier(ProductSurface.mark)
                            .accessibilityLabel("Satchel")
                        Text("Satchel")
                            .font(.headline.weight(.semibold))
                            .foregroundStyle(SatchelInk.ink)
                    }
                }
                if let backLabel, let onBack {
                    Button(action: onBack) {
                        Text(backLabel)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(SatchelInk.accent)
                    }
                    .buttonStyle(.plain)
                    .disabled(backDisabled)
                }
                Text(title)
                    .font(.largeTitle.weight(.semibold))
                    .foregroundStyle(SatchelInk.ink)
                    .accessibilityAddTraits(.isHeader)
            }
            .padding(.horizontal, 20)
            .padding(.top, 18)
            .padding(.bottom, 12)
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    content()
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 32)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(SatchelInk.canvas.ignoresSafeArea())
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(ProductSurface.identifier)
        .accessibilityValue(ProductSurface.container)
    }
}

struct ProductCard<Content: View>: View {
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            content()
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(SatchelInk.card)
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(SatchelInk.line, lineWidth: 1)
        )
    }
}

struct ProductField: View {
    let title: String
    @Binding var text: String
    var suppressesAutocorrection: Bool = false
    var secure: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(SatchelInk.secondary)
            Group {
                if secure {
                    SecureField(title, text: $text)
                } else {
                    TextField(title, text: $text)
                }
            }
                .textInputAutocapitalization(suppressesAutocorrection ? .never : .sentences)
                .autocorrectionDisabled(suppressesAutocorrection)
                .foregroundStyle(SatchelInk.ink)
                .padding(12)
                .background(SatchelInk.field)
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        }
    }
}

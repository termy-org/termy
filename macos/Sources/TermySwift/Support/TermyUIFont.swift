import SwiftUI

struct TermyUIFontModifier: ViewModifier {
    @ObservedObject private var configurationStore = TermyConfigurationStore.shared
    let size: CGFloat

    func body(content: Content) -> some View {
        let family = configurationStore.configuration.uiFontFamily
        content.font(.custom(family, size: size))
    }
}

extension View {
    func termyUIFont(size: CGFloat = 13) -> some View {
        modifier(TermyUIFontModifier(size: size))
    }
}

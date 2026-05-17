import SwiftUI

struct TerminalTopLoader: View {
    let progress: TerminalProgress

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Rectangle()
                    .fill(Color.clear)

                loader(proxy.size.width)
            }
        }
        .frame(height: 2)
        .opacity(progress.isVisible ? 1 : 0)
        .animation(.easeInOut(duration: 0.16), value: progress.isVisible)
    }

    @ViewBuilder
    private func loader(_ width: CGFloat) -> some View {
        if let fraction = progress.fraction {
            Rectangle()
                .fill(color)
                .frame(width: max(0, min(width, width * fraction)))
        } else if progress.isVisible {
            TimelineView(.animation(minimumInterval: 1.0 / 30.0)) { timeline in
                let phase = timeline.date.timeIntervalSinceReferenceDate
                    .truncatingRemainder(dividingBy: 1.2) / 1.2
                Rectangle()
                    .fill(color)
                    .frame(width: max(48, width * 0.22))
                    .offset(x: -max(48, width * 0.22) + (width + max(48, width * 0.22)) * phase)
            }
        }
    }

    private var color: Color {
        switch progress {
        case .error:
            return .red
        case .warning:
            return .orange
        case .clear, .inProgress, .indeterminate:
            return .accentColor
        }
    }
}

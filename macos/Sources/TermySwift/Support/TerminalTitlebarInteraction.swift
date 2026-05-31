import AppKit
import SwiftUI

@MainActor
enum TerminalTitlebarInteraction {
    static let trafficLightReservedWidth: CGFloat = 78
    static let fallbackTitlebarHeight: CGFloat = 52
    static let minimumTitlebarHeight: CGFloat = 28

    static func titlebarInteractionHeight(for window: NSWindow?) -> CGFloat {
        guard let window else {
            return fallbackTitlebarHeight
        }

        let windowFrame = window.frame
        let contentLayout = window.contentLayoutRect
        let measured = windowFrame.height
            - contentLayout.height
            - (windowFrame.origin.y - contentLayout.origin.y)
        return max(minimumTitlebarHeight, measured)
    }

    static func performTitlebarDoubleClickAction(on window: NSWindow) {
        switch titlebarDoubleClickAction() {
        case .minimize:
            window.miniaturize(nil)
        case .maximize:
            window.zoom(nil)
        case .fill:
            window.toggleFullScreen(nil)
        }
    }

    private enum TitlebarDoubleClickAction {
        case minimize
        case maximize
        case fill
    }

    private static func titlebarDoubleClickAction() -> TitlebarDoubleClickAction {
        let raw = UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick") ?? "Maximize"
        switch raw {
        case "Minimize":
            return .minimize
        case "Fill":
            return .fill
        default:
            return .maximize
        }
    }
}

struct TerminalTitlebarInteractionView: NSViewRepresentable {
    var titlebarHeight: CGFloat

    func makeNSView(context: Context) -> TitlebarInteractionCaptureView {
        let view = TitlebarInteractionCaptureView()
        view.titlebarHeight = titlebarHeight
        return view
    }

    func updateNSView(_ view: TitlebarInteractionCaptureView, context: Context) {
        view.titlebarHeight = titlebarHeight
    }
}

final class TitlebarInteractionCaptureView: NSView {
    var titlebarHeight: CGFloat = TerminalTitlebarInteraction.fallbackTitlebarHeight

    override var isOpaque: Bool {
        false
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard bounds.contains(point) else {
            return nil
        }
        if point.x < TerminalTitlebarInteraction.trafficLightReservedWidth {
            return nil
        }
        return self
    }

    override func mouseDown(with event: NSEvent) {
        guard let window else {
            return
        }
        if event.clickCount >= 2 {
            TerminalTitlebarInteraction.performTitlebarDoubleClickAction(on: window)
            return
        }
        window.performDrag(with: event)
    }
}

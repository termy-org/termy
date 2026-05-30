import AppKit
import Combine
import Dispatch
import Foundation
import ObjectiveC
import SwiftUI

private let actionSelect: Int32 = 1
private let actionNew: Int32 = 2

public typealias GPUIAppKitNativeTabsCallback = @convention(c) (
    UnsafeMutableRawPointer?,
    Int32,
    UnsafePointer<CChar>?
) -> Void

private struct NativeTitlebarTabsPayload: Decodable {
    let tabs: [NativeTitlebarTabPayload]
    let selectedId: String?
    let height: Double?
    let showsAddButton: Bool?
}

private struct NativeTitlebarTabPayload: Decodable {
    let id: String
    let title: String
    let isSelected: Bool?
    let isPinned: Bool?
    let isLoading: Bool?
}

private struct NativeTitlebarTabItem: Identifiable, Equatable {
    let id: String
    let title: String
    let isPinned: Bool
    let isLoading: Bool
}

private final class NativeTitlebarTabsModel: ObservableObject {
    @Published var tabs: [NativeTitlebarTabItem] = []
    @Published var selectedId: String = ""
    @Published var height: CGFloat = 30
    @Published var showsAddButton: Bool = true

    private var callback: GPUIAppKitNativeTabsCallback?
    private var callbackContext: UnsafeMutableRawPointer?

    func update(
        payload: NativeTitlebarTabsPayload,
        callback: GPUIAppKitNativeTabsCallback?,
        context: UnsafeMutableRawPointer?
    ) {
        self.tabs = payload.tabs.map {
            NativeTitlebarTabItem(
                id: $0.id,
                title: $0.title,
                isPinned: $0.isPinned ?? false,
                isLoading: $0.isLoading ?? false
            )
        }
        self.selectedId = payload.selectedId
            ?? payload.tabs.first(where: { $0.isSelected == true })?.id
            ?? payload.tabs.first?.id
            ?? ""
        self.height = CGFloat(payload.height ?? 30)
        self.showsAddButton = payload.showsAddButton ?? true
        self.callback = callback
        self.callbackContext = context
    }

    func selectTab(id: String) {
        selectedId = id
        send(actionSelect, tabId: id)
    }

    func createTab() {
        send(actionNew, tabId: nil)
    }

    private func send(_ action: Int32, tabId: String?) {
        guard let callback else {
            return
        }

        if let tabId {
            tabId.withCString { callback(callbackContext, action, $0) }
        } else {
            callback(callbackContext, action, nil)
        }
    }
}

private struct NativeTitlebarTabsView: View {
    @ObservedObject var model: NativeTitlebarTabsModel

    var body: some View {
        HStack(spacing: 6) {
            if !model.tabs.isEmpty {
                Picker(
                    "",
                    selection: Binding(
                        get: { model.selectedId },
                        set: { model.selectTab(id: $0) }
                    )
                ) {
                    ForEach(model.tabs) { tab in
                        Text(tab.title)
                            .lineLimit(1)
                            .tag(tab.id)
                    }
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .frame(minWidth: 160)
            }

            if model.showsAddButton {
                Button(action: model.createTab) {
                    Image(systemName: "plus")
                        .imageScale(.small)
                }
                .buttonStyle(.borderless)
                .accessibilityLabel("New Tab")
            }
        }
        .padding(.horizontal, 8)
        .frame(height: model.height)
    }
}

private final class NativeTitlebarTabsAttachment: NSObject {
    let model: NativeTitlebarTabsModel
    let controller: NSTitlebarAccessoryViewController

    init(
        payload: NativeTitlebarTabsPayload,
        callback: GPUIAppKitNativeTabsCallback?,
        context: UnsafeMutableRawPointer?
    ) {
        self.model = NativeTitlebarTabsModel()
        self.controller = NSTitlebarAccessoryViewController()
        super.init()

        model.update(payload: payload, callback: callback, context: context)

        let hostingView = NSHostingView(rootView: NativeTitlebarTabsView(model: model))
        hostingView.frame = NSRect(x: 0, y: 0, width: 360, height: model.height)
        hostingView.setContentHuggingPriority(.defaultLow, for: .horizontal)
        hostingView.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        controller.view = hostingView
        controller.layoutAttribute = .bottom
        controller.fullScreenMinHeight = model.height
    }

    func update(
        payload: NativeTitlebarTabsPayload,
        callback: GPUIAppKitNativeTabsCallback?,
        context: UnsafeMutableRawPointer?
    ) {
        model.update(payload: payload, callback: callback, context: context)
        controller.fullScreenMinHeight = model.height
        controller.view.frame.size.height = model.height
    }
}

private var attachmentAssociationKey: UInt8 = 0

private func runOnMain(_ body: @escaping () -> Int32) -> Int32 {
    if Thread.isMainThread {
        return body()
    }

    var result: Int32 = -10
    DispatchQueue.main.sync {
        result = body()
    }
    return result
}

private func decodePayload(_ payloadJson: UnsafePointer<CChar>?) -> NativeTitlebarTabsPayload? {
    guard let payloadJson else {
        return nil
    }

    let json = String(cString: payloadJson)
    guard let data = json.data(using: .utf8) else {
        return nil
    }

    return try? JSONDecoder().decode(NativeTitlebarTabsPayload.self, from: data)
}

@_cdecl("gpui_native_appkit_install_or_update_titlebar_tabs")
public func gpui_native_appkit_install_or_update_titlebar_tabs(
    _ nsViewRaw: UnsafeMutableRawPointer?,
    _ payloadJson: UnsafePointer<CChar>?,
    _ callback: GPUIAppKitNativeTabsCallback?,
    _ callbackContext: UnsafeMutableRawPointer?
) -> Int32 {
    runOnMain {
        guard let nsViewRaw else {
            return -1
        }
        guard let payload = decodePayload(payloadJson) else {
            return -3
        }

        let view = Unmanaged<NSView>.fromOpaque(nsViewRaw).takeUnretainedValue()
        guard let window = view.window else {
            return -2
        }

        if let existing = objc_getAssociatedObject(window, &attachmentAssociationKey)
            as? NativeTitlebarTabsAttachment
        {
            existing.update(payload: payload, callback: callback, context: callbackContext)
        } else {
            let attachment = NativeTitlebarTabsAttachment(
                payload: payload,
                callback: callback,
                context: callbackContext
            )
            window.titleVisibility = .hidden
            window.titlebarAppearsTransparent = true
            window.addTitlebarAccessoryViewController(attachment.controller)
            objc_setAssociatedObject(
                window,
                &attachmentAssociationKey,
                attachment,
                .OBJC_ASSOCIATION_RETAIN_NONATOMIC
            )
        }

        return 0
    }
}

@_cdecl("gpui_native_appkit_remove_titlebar_tabs")
public func gpui_native_appkit_remove_titlebar_tabs(
    _ nsViewRaw: UnsafeMutableRawPointer?
) -> Int32 {
    runOnMain {
        guard let nsViewRaw else {
            return -1
        }

        let view = Unmanaged<NSView>.fromOpaque(nsViewRaw).takeUnretainedValue()
        guard let window = view.window else {
            return -2
        }

        if let existing = objc_getAssociatedObject(window, &attachmentAssociationKey)
            as? NativeTitlebarTabsAttachment
        {
            if let index = window.titlebarAccessoryViewControllers.firstIndex(of: existing.controller) {
                window.removeTitlebarAccessoryViewController(at: index)
            }
            objc_setAssociatedObject(window, &attachmentAssociationKey, nil, .OBJC_ASSOCIATION_ASSIGN)
        }

        return 0
    }
}

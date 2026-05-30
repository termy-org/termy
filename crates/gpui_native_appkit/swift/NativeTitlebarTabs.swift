import AppKit
import Dispatch
import Foundation
import ObjectiveC

public typealias GPUIAppKitNewWindowTabCallback = @convention(c) (
    UnsafeMutableRawPointer?
) -> Void

private final class NativeWindowTabResponder: NSResponder {
    var callback: GPUIAppKitNewWindowTabCallback?
    var callbackContext: UnsafeMutableRawPointer?

    func update(
        callback: GPUIAppKitNewWindowTabCallback?,
        context: UnsafeMutableRawPointer?
    ) {
        self.callback = callback
        self.callbackContext = context
    }

    @IBAction override func newWindowForTab(_ sender: Any?) {
        guard let callback else {
            return
        }
        let context = callbackContext
        DispatchQueue.main.async {
            callback(context)
        }
    }
}

private var nativeWindowTabResponderAssociationKey: UInt8 = 0

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

private func windowFromView(_ nsViewRaw: UnsafeMutableRawPointer?) -> NSWindow? {
    guard let nsViewRaw else {
        return nil
    }

    let view = Unmanaged<NSView>.fromOpaque(nsViewRaw).takeUnretainedValue()
    return view.window
}

private func addWindowToTabGroup(anchorWindow: NSWindow, window: NSWindow) {
    if window.tabbingIdentifier.isEmpty {
        window.tabbingIdentifier = anchorWindow.tabbingIdentifier
    }
    anchorWindow.addTabbedWindow(window, ordered: .above)
    anchorWindow.tabGroup?.selectedWindow = window
    showTabBarIfHidden(on: anchorWindow)
    showTabBarIfHidden(on: window)
}

private func showTabBarIfHidden(on window: NSWindow) {
    if window.tabGroup?.isTabBarVisible == true {
        return
    }
    window.toggleTabBar(nil)
}

private func scheduleShowTabBarIfHidden(on window: NSWindow) {
    DispatchQueue.main.async {
        showTabBarIfHidden(on: window)
    }
}

private func installNewWindowForTabResponder(
    on window: NSWindow,
    callback: GPUIAppKitNewWindowTabCallback?,
    context: UnsafeMutableRawPointer?
) {
    guard callback != nil else {
        return
    }

    if let responder = objc_getAssociatedObject(
        window,
        &nativeWindowTabResponderAssociationKey
    ) as? NativeWindowTabResponder {
        responder.update(callback: callback, context: context)
        return
    }

    let responder = NativeWindowTabResponder()
    responder.update(callback: callback, context: context)
    responder.nextResponder = window.nextResponder
    window.nextResponder = responder
    objc_setAssociatedObject(
        window,
        &nativeWindowTabResponderAssociationKey,
        responder,
        .OBJC_ASSOCIATION_RETAIN_NONATOMIC
    )
}

@_cdecl("gpui_native_appkit_configure_window_tabbing")
public func gpui_native_appkit_configure_window_tabbing(
    _ nsViewRaw: UnsafeMutableRawPointer?,
    _ identifierRaw: UnsafePointer<CChar>?,
    _ titleRaw: UnsafePointer<CChar>?,
    _ newWindowTabCallback: GPUIAppKitNewWindowTabCallback?,
    _ newWindowTabCallbackContext: UnsafeMutableRawPointer?
) -> Int32 {
    runOnMain {
        guard let window = windowFromView(nsViewRaw) else {
            return -2
        }
        guard let identifierRaw else {
            return -3
        }

        NSWindow.allowsAutomaticWindowTabbing = true
        window.tabbingMode = .preferred
        window.tabbingIdentifier = NSWindow.TabbingIdentifier(String(cString: identifierRaw))
        window.tab.title = titleRaw.map { String(cString: $0) } ?? window.title
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.titlebarSeparatorStyle = .none
        scheduleShowTabBarIfHidden(on: window)
        installNewWindowForTabResponder(
            on: window,
            callback: newWindowTabCallback,
            context: newWindowTabCallbackContext
        )

        return 0
    }
}

@_cdecl("gpui_native_appkit_add_window_to_tab_group")
public func gpui_native_appkit_add_window_to_tab_group(
    _ anchorViewRaw: UnsafeMutableRawPointer?,
    _ windowViewRaw: UnsafeMutableRawPointer?
) -> Int32 {
    runOnMain {
        guard let anchorWindow = windowFromView(anchorViewRaw),
              let window = windowFromView(windowViewRaw)
        else {
            return -2
        }

        // `addTabbedWindow` can synchronously drive AppKit layout/display. When
        // invoked from a GPUI action callback, that reenters GPUI while its app
        // state is already mutably borrowed and aborts with "RefCell already
        // borrowed". Queue the imperative AppKit mutation onto the next main
        // turn so GPUI has returned from the current dispatch first.
        DispatchQueue.main.async {
            addWindowToTabGroup(anchorWindow: anchorWindow, window: window)
        }
        return 0
    }
}

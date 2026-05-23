import AppKit

enum TerminalSurfaceContextMenu {
    static func make(canCopy: Bool, canPaste: Bool, target: AnyObject) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false

        menu.addItem(item(
            title: "Copy",
            action: #selector(KeyboardCaptureView.copyFromTerminalContextMenu(_:)),
            keyEquivalent: "c",
            target: target,
            isEnabled: canCopy
        ))
        menu.addItem(item(
            title: "Paste",
            action: #selector(KeyboardCaptureView.pasteFromTerminalContextMenu(_:)),
            keyEquivalent: "v",
            target: target,
            isEnabled: canPaste
        ))
        menu.addItem(.separator())
        menu.addItem(item(
            title: "Clear Scrollback",
            action: #selector(KeyboardCaptureView.clearBufferFromTerminalContextMenu(_:)),
            keyEquivalent: "k",
            target: target
        ))
        menu.addItem(item(
            title: "Search",
            action: #selector(KeyboardCaptureView.showSearchFromTerminalContextMenu(_:)),
            keyEquivalent: "f",
            target: target
        ))

        return menu
    }

    private static func item(
        title: String,
        action: Selector,
        keyEquivalent: String,
        target: AnyObject,
        isEnabled: Bool = true
    ) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: keyEquivalent)
        item.target = target
        item.isEnabled = isEnabled
        return item
    }
}

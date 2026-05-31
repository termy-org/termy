import AppKit
import SwiftUI

struct NativeTabChromeView: View {
    weak var window: NSWindow?
    let configuration: TermyAppConfiguration

    @State private var hoveredTabID: ObjectIdentifier?
    @State private var modifierFlags: NSEvent.ModifierFlags = []
    @State private var modifierMonitor: Any?
    @State private var refreshToken = 0

    private var tabs: [NativeTabDescriptor] {
        _ = refreshToken
        return NativeTabWindowManager.shared.tabDescriptors(for: window)
    }

    var body: some View {
        if shouldRender {
            switch configuration.native.tabBarPosition {
            case .top:
                horizontalChrome
            case .right:
                verticalChrome
            }
        }
    }

    private var shouldRender: Bool {
        !tabs.isEmpty && (!configuration.native.autoHideTabbar || tabs.count > 1)
    }

    private var horizontalChrome: some View {
        HStack(spacing: 6) {
            if configuration.native.showTermyInTitlebar {
                Text("Termy")
                    .font(uiFont(size: 12, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .padding(.leading, 10)
            }

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 4) {
                    ForEach(tabs) { tab in
                        tabButton(tab, axis: .horizontal)
                    }
                }
                .padding(.horizontal, 6)
            }

            Button {
                NativeTabWindowManager.shared.openNativeTab()
            } label: {
                Image(systemName: "plus")
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .help("New Tab")
            .padding(.trailing, 8)
        }
        .frame(height: 34)
        .background(.bar)
        .overlay(alignment: .bottom) {
            Divider()
        }
        .modifierFlagsTracking($modifierFlags, monitor: $modifierMonitor)
        .onReceive(NotificationCenter.default.publisher(for: .termyNativeTabsChanged)) { _ in
            refreshToken &+= 1
        }
    }

    private var verticalChrome: some View {
        VStack(spacing: 6) {
            if configuration.native.showTermyInTitlebar {
                Text("Termy")
                    .font(uiFont(size: 12, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .padding(.top, 10)
            }

            Button {
                NativeTabWindowManager.shared.openNativeTab()
            } label: {
                Image(systemName: "plus")
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.plain)
            .help("New Tab")

            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(spacing: 5) {
                    ForEach(tabs) { tab in
                        tabButton(tab, axis: .vertical)
                    }
                }
                .padding(6)
            }
        }
        .frame(width: 164)
        .background(.bar)
        .overlay(alignment: .leading) {
            Divider()
        }
        .modifierFlagsTracking($modifierFlags, monitor: $modifierMonitor)
        .onReceive(NotificationCenter.default.publisher(for: .termyNativeTabsChanged)) { _ in
            refreshToken &+= 1
        }
    }

    private func tabButton(_ tab: NativeTabDescriptor, axis: Axis) -> some View {
        HStack(spacing: 6) {
            if shouldShowSwitchHint(for: tab) {
                Text("\(tab.index + 1)")
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .frame(width: 14)
            }

            Text(tab.title)
                .font(uiFont(size: 12, weight: tab.isSelected ? .semibold : .regular))
                .lineLimit(1)

            Spacer(minLength: 4)

            if shouldShowCloseButton(for: tab) {
                Button {
                    NativeTabWindowManager.shared.closeNativeTab(tab)
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 10, weight: .semibold))
                        .frame(width: 16, height: 16)
                }
                .buttonStyle(.plain)
                .help("Close Tab")
            }
        }
        .padding(.horizontal, 8)
        .frame(width: tabWidth(for: tab, axis: axis), height: 24)
        .background(tab.isSelected ? Color.accentColor.opacity(0.16) : Color.clear, in: RoundedRectangle(cornerRadius: 6))
        .contentShape(Rectangle())
        .onHover { hovering in
            hoveredTabID = hovering ? tab.id : nil
        }
        .onTapGesture {
            NativeTabWindowManager.shared.selectNativeTab(tab)
        }
    }

    private func shouldShowCloseButton(for tab: NativeTabDescriptor) -> Bool {
        switch configuration.native.tabCloseVisibility {
        case .always:
            return true
        case .hover:
            return hoveredTabID == tab.id
        case .activeHover:
            return tab.isSelected || hoveredTabID == tab.id
        }
    }

    private func shouldShowSwitchHint(for tab: NativeTabDescriptor) -> Bool {
        configuration.native.tabSwitchModifierHints
            && modifierFlags.contains(.command)
            && tab.index < 9
    }

    private func tabWidth(for tab: NativeTabDescriptor, axis: Axis) -> CGFloat? {
        if axis == .vertical {
            return nil
        }

        switch configuration.native.tabWidthMode {
        case .stable:
            return 148
        case .activeGrow:
            return tab.isSelected ? 210 : 126
        case .activeGrowSticky:
            return tab.isSelected ? 210 : 148
        case .uniform:
            return 156
        }
    }

    private func uiFont(size: CGFloat, weight: Font.Weight) -> Font {
        .custom(configuration.uiFontFamily, size: size).weight(weight)
    }
}

private extension View {
    func modifierFlagsTracking(
        _ flags: Binding<NSEvent.ModifierFlags>,
        monitor: Binding<Any?>
    ) -> some View {
        onAppear {
            flags.wrappedValue = NSEvent.modifierFlags
            guard monitor.wrappedValue == nil else {
                return
            }
            monitor.wrappedValue = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) { event in
                flags.wrappedValue = event.modifierFlags
                return event
            }
        }
        .onDisappear {
            if let activeMonitor = monitor.wrappedValue {
                NSEvent.removeMonitor(activeMonitor)
                monitor.wrappedValue = nil
            }
        }
    }
}

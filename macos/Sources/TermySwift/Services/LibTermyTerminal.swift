import CTermy
import Foundation

enum LibTermyError: Error, CustomStringConvertible {
    case ffi(String, TermyFfiStatus)
    case missingTerminal
    case missingCells

    var description: String {
        switch self {
        case let .ffi(operation, status):
            return "\(operation) failed with status \(status.rawValue)"
        case .missingTerminal:
            return "libtermy did not return a terminal handle"
        case .missingCells:
            return "libtermy returned a frame without cells"
        }
    }
}

final class LibTermyTerminal {
    private var handle: OpaquePointer?
    private var configHandle: OpaquePointer?

    let configSummary: String
    let renderConfig: TerminalRenderConfig

    init(cols: UInt16 = 96, rows: UInt16 = 28, loadUserConfig: Bool = true) throws {
        var size = termy_size_default()
        size.cols = cols
        size.rows = rows
        let startupCommand = Self.startupCommand()
        let config = try loadUserConfig ? Self.loadDefaultConfig() : nil
        configHandle = config
        configSummary = Self.configSummary(for: config)
        renderConfig = try Self.renderConfig(for: config)
        let workingDirectory = try Self.workingDirectory(for: config)

        var terminal: OpaquePointer?
        let startupCommandBytes = startupCommand ?? []
        let workingDirectoryBytes = workingDirectory.map { Array($0.utf8) } ?? []
        let status = startupCommandBytes.withUnsafeBufferPointer { startupBuffer in
            workingDirectoryBytes.withUnsafeBufferPointer { workingDirectoryBuffer in
                var options = TermyFfiTerminalOptions(
                    config: config,
                    working_directory_ptr: workingDirectoryBuffer.baseAddress,
                    working_directory_len: workingDirectoryBuffer.count,
                    startup_command_ptr: startupBuffer.baseAddress,
                    startup_command_len: startupBuffer.count,
                    env_vars_ptr: nil,
                    env_vars_len: 0
                )
                return termy_terminal_new_with_options(size, &options, &terminal)
            }
        }
        try Self.requireOK("termy_terminal_new_with_options", status)

        guard let terminal else {
            throw LibTermyError.missingTerminal
        }
        handle = terminal
    }

    deinit {
        if let handle {
            _ = termy_terminal_free(handle)
        }
        if let configHandle {
            _ = termy_config_free(configHandle)
        }
    }

    func write(_ bytes: [UInt8]) throws {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }
        let status = bytes.withUnsafeBufferPointer { buffer in
            termy_terminal_write(handle, buffer.baseAddress, buffer.count)
        }
        try Self.requireOK("termy_terminal_write", status)
    }

    func encodeKey(_ keyInput: TerminalKeyInput) throws -> [UInt8]? {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }

        let keyBytes = Array(keyInput.key.utf8)
        let keyCharBytes = keyInput.keyChar.map { Array($0.utf8) } ?? []
        var outBytes = TermyFfiBytes()

        let status = keyBytes.withUnsafeBufferPointer { keyBuffer in
            keyCharBytes.withUnsafeBufferPointer { keyCharBuffer in
                var ffiKeystroke = TermyFfiKeystroke(
                    control: keyInput.control,
                    alt: keyInput.alt,
                    shift: keyInput.shift,
                    platform: keyInput.platform,
                    function: keyInput.function,
                    key_ptr: keyBuffer.baseAddress,
                    key_len: keyBuffer.count,
                    key_char_ptr: keyCharBuffer.baseAddress,
                    key_char_len: keyCharBuffer.count,
                    event_kind: keyInput.eventKind
                )
                return termy_terminal_encode_key(handle, &ffiKeystroke, &outBytes)
            }
        }
        try Self.requireOK("termy_terminal_encode_key", status)
        defer {
            if outBytes.ptr != nil {
                _ = termy_buffer_free(outBytes)
            }
        }

        guard let ptr = outBytes.ptr, outBytes.len > 0 else {
            return nil
        }
        return Array(UnsafeBufferPointer(start: ptr, count: Int(outBytes.len)))
    }

    func resize(cols: UInt16, rows: UInt16, cellWidth: Float, cellHeight: Float) throws {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }
        var size = termy_size_default()
        size.cols = cols
        size.rows = rows
        size.cell_width = cellWidth
        size.cell_height = cellHeight
        try Self.requireOK("termy_terminal_resize", termy_terminal_resize(handle, size))
    }

    func scrollDisplay(deltaLines: Int32) throws -> Bool {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }

        var changed = false
        try Self.requireOK(
            "termy_terminal_scroll_display",
            termy_terminal_scroll_display(handle, deltaLines, &changed)
        )
        return changed
    }

    func scrollToBottom() throws -> Bool {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }

        var changed = false
        try Self.requireOK(
            "termy_terminal_scroll_to_bottom",
            termy_terminal_scroll_to_bottom(handle, &changed)
        )
        return changed
    }

    func drainEvents() throws -> [TerminalRuntimeEvent] {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }
        var batch = TermyFfiEventBatch()
        try Self.requireOK(
            "termy_terminal_drain_events",
            termy_terminal_drain_events(handle, &batch)
        )
        defer {
            _ = termy_event_batch_free(&batch)
        }

        guard let eventsPtr = batch.events_ptr else {
            return []
        }
        return UnsafeBufferPointer(start: eventsPtr, count: Int(batch.events_len))
            .compactMap(Self.event(from:))
    }

    func takeDamage() throws -> Bool {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }

        var damage = TermyFfiDamage()
        try Self.requireOK(
            "termy_terminal_take_damage",
            termy_terminal_take_damage(handle, &damage)
        )
        let hasDamage = damage.kind != 0 || damage.spans_len > 0
        _ = termy_damage_free(&damage)
        return hasDamage
    }

    func snapshot() throws -> TerminalFrame {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }

        var frame = TermyFfiFrame()
        try Self.requireOK("termy_terminal_snapshot", termy_terminal_snapshot(handle, &frame))
        defer {
            _ = termy_frame_free(&frame)
        }

        guard let cellsPtr = frame.cells_ptr else {
            throw LibTermyError.missingCells
        }

        let cells = UnsafeBufferPointer(start: cellsPtr, count: Int(frame.cells_len))
            .map(Self.cell(from:))
        let cursor = frame.cursor.visible
            ? TerminalCursor(
                col: Int(frame.cursor.col),
                row: Int(frame.cursor.row),
                style: frame.cursor.style
            )
            : nil

        return TerminalFrame(
            cols: Int(frame.cols),
            rows: Int(frame.rows),
            cells: cells,
            cursor: cursor,
            displayOffset: Int(frame.display_offset),
            historySize: Int(frame.history_size)
        )
    }

    func search(_ query: String) throws -> [TerminalSearchMatch] {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }

        var batch = TermyFfiSearchBatch()
        let status = Array(query.utf8).withUnsafeBufferPointer { buffer in
            termy_terminal_search(handle, buffer.baseAddress, buffer.count, &batch)
        }
        try Self.requireOK("termy_terminal_search", status)
        defer {
            _ = termy_search_batch_free(&batch)
        }

        guard let matchesPtr = batch.matches_ptr else {
            return []
        }
        return UnsafeBufferPointer(start: matchesPtr, count: Int(batch.matches_len))
            .map { match in
                TerminalSearchMatch(
                    row: Int(match.row),
                    startCol: Int(match.start_col),
                    endCol: Int(match.end_col),
                    line: Self.string(from: match.line) ?? ""
                )
            }
    }

    private static func requireOK(_ operation: String, _ status: TermyFfiStatus) throws {
        guard status == TERMY_FFI_OK else {
            throw LibTermyError.ffi(operation, status)
        }
    }

    private static func loadDefaultConfig() throws -> OpaquePointer? {
        var config: OpaquePointer?
        try requireOK("termy_config_load_default", termy_config_load_default(&config))
        return config
    }

    private static func startupCommand() -> [UInt8]? {
        if ProcessInfo.processInfo.environment["TERMY_SWIFT_EXAMPLE_EXIT_AFTER_RENDER"] == "1" {
            return Array("printf 'libtermy SwiftUI smoke\\n'".utf8)
        }
        return nil
    }

    private static func configSummary(for config: OpaquePointer?) -> String {
        guard let config else {
            return "config: off"
        }

        let diagnostics = Int(termy_config_diagnostic_count(config))
        let suffix = diagnostics == 0 ? "" : " (\(diagnostics) diagnostics)"
        if termy_config_loaded_from_disk(config) {
            return "config: loaded\(suffix)"
        }
        return "config: defaults\(suffix)"
    }

    private static func renderConfig(for config: OpaquePointer?) throws -> TerminalRenderConfig {
        guard let config else {
            return .default
        }

        var renderConfig = TermyFfiRenderConfig()
        try requireOK(
            "termy_config_render_config",
            termy_config_render_config(config, &renderConfig)
        )
        defer {
            _ = termy_render_config_free(&renderConfig)
        }
        return TerminalRenderConfig(renderConfig)
    }

    private static func workingDirectory(for config: OpaquePointer?) throws -> String? {
        guard let config else {
            return nil
        }

        var bytes = TermyFfiBytes()
        try requireOK(
            "termy_config_working_directory",
            termy_config_working_directory(config, &bytes)
        )
        defer {
            if bytes.ptr != nil {
                _ = termy_buffer_free(bytes)
            }
        }

        guard let ptr = bytes.ptr, bytes.len > 0 else {
            return nil
        }
        let buffer = UnsafeBufferPointer(start: ptr, count: Int(bytes.len))
        let value = String(decoding: buffer, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }

    private static func event(from event: TermyFfiEvent) -> TerminalRuntimeEvent? {
        switch event.kind {
        case UInt32(TERMY_FFI_EVENT_WAKEUP.rawValue):
            return .wakeup
        case UInt32(TERMY_FFI_EVENT_TITLE.rawValue):
            return .title(string(from: event.payload) ?? "")
        case UInt32(TERMY_FFI_EVENT_RESET_TITLE.rawValue):
            return .resetTitle
        case UInt32(TERMY_FFI_EVENT_BELL.rawValue):
            return .bell
        case UInt32(TERMY_FFI_EVENT_EXIT.rawValue):
            return .exit
        case UInt32(TERMY_FFI_EVENT_CLIPBOARD_STORE.rawValue):
            return .clipboardStore(string(from: event.payload) ?? "")
        case UInt32(TERMY_FFI_EVENT_SHELL_PROMPT_START.rawValue):
            return .shellPromptStart
        case UInt32(TERMY_FFI_EVENT_SHELL_COMMAND_START.rawValue):
            return .shellCommandStart
        case UInt32(TERMY_FFI_EVENT_SHELL_COMMAND_EXECUTING.rawValue):
            return .shellCommandExecuting
        case UInt32(TERMY_FFI_EVENT_SHELL_COMMAND_FINISHED.rawValue):
            return .shellCommandFinished(event.exit_code >= 0 ? event.exit_code : nil)
        case UInt32(TERMY_FFI_EVENT_PROGRESS.rawValue):
            return .progress(TerminalProgress(
                state: event.progress_state,
                value: event.progress_value
            ))
        case UInt32(TERMY_FFI_EVENT_WORKING_DIRECTORY.rawValue):
            return .workingDirectory(string(from: event.payload) ?? "")
        default:
            return nil
        }
    }

    private static func cell(from ffiCell: TermyFfiCell) -> TerminalCell {
        TerminalCell(
            col: Int(ffiCell.col),
            row: Int(ffiCell.row),
            character: character(from: ffiCell.codepoint),
            foreground: TerminalRGBA(ffiCell.fg),
            background: TerminalRGBA(ffiCell.bg),
            usesTerminalDefaultBackground: ffiCell.uses_terminal_default_bg,
            renderText: ffiCell.render_text,
            bold: ffiCell.bold
        )
    }

    private static func character(from codepoint: UInt32) -> Character {
        UnicodeScalar(codepoint).map(Character.init) ?? " "
    }

    private static func string(from bytes: TermyFfiBytes) -> String? {
        guard let ptr = bytes.ptr, bytes.len > 0 else {
            return nil
        }
        let buffer = UnsafeBufferPointer(start: ptr, count: Int(bytes.len))
        return String(decoding: buffer, as: UTF8.self)
    }
}

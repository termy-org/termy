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

    init(cols: UInt16 = 96, rows: UInt16 = 28, loadUserConfig: Bool = true) throws {
        var size = termy_size_default()
        size.cols = cols
        size.rows = rows
        let startupCommand = Self.startupCommand()
        let config = try loadUserConfig ? Self.loadDefaultConfig() : nil
        configHandle = config
        configSummary = Self.configSummary(for: config)

        var terminal: OpaquePointer?
        let status: TermyFfiStatus
        if let startupCommand {
            status = startupCommand.withUnsafeBufferPointer { buffer in
                if let config {
                    return termy_terminal_new_with_config(
                        size,
                        config,
                        buffer.baseAddress,
                        buffer.count,
                        &terminal
                    )
                } else {
                    return termy_terminal_new(
                        size,
                        buffer.baseAddress,
                        buffer.count,
                        &terminal
                    )
                }
            }
        } else {
            if let config {
                status = termy_terminal_new_with_config(
                    size,
                    config,
                    nil,
                    0,
                    &terminal
                )
            } else {
                status = termy_terminal_new(
                    size,
                    nil,
                    0,
                    &terminal
                )
            }
        }
        try Self.requireOK("termy_terminal_new", status)

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

    func drainEvents() throws {
        guard let handle else {
            throw LibTermyError.missingTerminal
        }
        var batch = TermyFfiEventBatch()
        try Self.requireOK(
            "termy_terminal_drain_events",
            termy_terminal_drain_events(handle, &batch)
        )
        _ = termy_event_batch_free(&batch)
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
            cursor: cursor
        )
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

    private static func cell(from ffiCell: TermyFfiCell) -> TerminalCell {
        TerminalCell(
            col: Int(ffiCell.col),
            row: Int(ffiCell.row),
            character: character(from: ffiCell.codepoint),
            foreground: TerminalRGBA(ffiCell.fg),
            background: TerminalRGBA(ffiCell.bg),
            renderText: ffiCell.render_text,
            bold: ffiCell.bold
        )
    }

    private static func character(from codepoint: UInt32) -> Character {
        UnicodeScalar(codepoint).map(Character.init) ?? " "
    }
}

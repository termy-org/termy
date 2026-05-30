import CTermy
import Foundation

enum TermyFfiError: Error, CustomStringConvertible {
    case ffi(String, TermyFfiStatus)

    var description: String {
        switch self {
        case let .ffi(operation, status):
            return "\(operation) failed with status \(status.rawValue)"
        }
    }
}

enum TermyFfiBridge {
    static func requireOK(_ operation: String, _ status: TermyFfiStatus) throws {
        guard status == TERMY_FFI_OK else {
            throw TermyFfiError.ffi(operation, status)
        }
    }

    static func string(
        from bytes: TermyFfiBytes,
        trimmingWhitespaceAndNewlines shouldTrim: Bool = false
    ) -> String? {
        guard let ptr = bytes.ptr, bytes.len > 0 else {
            return nil
        }

        let buffer = UnsafeBufferPointer(start: ptr, count: Int(bytes.len))
        let value = String(decoding: buffer, as: UTF8.self)
        return shouldTrim ? value.trimmingCharacters(in: .whitespacesAndNewlines) : value
    }
}

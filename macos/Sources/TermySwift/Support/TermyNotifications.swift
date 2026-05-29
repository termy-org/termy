import Foundation

/// Posted whenever a setting is written to disk so open terminals can live-apply
/// appearance changes.
extension Notification.Name {
    static let termySettingsChanged = Notification.Name("TermySettingsChanged")
}

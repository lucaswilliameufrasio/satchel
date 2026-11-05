import Foundation

/// Where the words on screen come from.
///
/// The app follows the system language. English is the development language and
/// Brazilian Portuguese is the one translation, both in `Localizable.xcstrings`.
/// There is no in-app switch: the app leaves this on the system's choice and only
/// tests pin a language with `use(_:)`, so labels built outside a view stay testable.
enum Copy {
    private(set) static var bundle: Bundle = .main
    private(set) static var locale: Locale = .autoupdatingCurrent

    /// Pin `language` (`"en"` or `"pt-BR"`). `nil` goes back to the system's choice.
    static func use(_ language: String?) {
        guard let language,
              let path = Bundle.main.path(forResource: language, ofType: "lproj"),
              let pinned = Bundle(path: path)
        else {
            bundle = .main
            locale = .autoupdatingCurrent
            return
        }
        bundle = pinned
        locale = Locale(identifier: language)
    }
}

extension Bundle {
    /// The bundle every `String(localized:)` in the app reads from.
    static var copy: Bundle { Copy.bundle }
}

extension Locale {
    /// The locale that formats numbers and picks plural forms in that copy.
    static var copy: Locale { Copy.locale }
}

.pragma library

// Airmail's stamp avatars: red or blue by sender, never by row position.
// The tint comes from a hash of the address (or name) the caller has, so a
// sender keeps one colour in every list. Pure, no Qt.

// The address in a sender line: what is inside the last <...>, else the
// whole line, trimmed and lowercased ("Ann <a@x.org>" and "a@x.org" agree).
function addressOf(from) {
    var s = String(from)
    var m = s.match(/<([^<>]*)>\s*$/)
    return (m ? m[1] : s).trim().toLowerCase()
}

// true: the blue stamp; false: the red one. Case-insensitive.
function isBlue(address) {
    var h = 2166136261
    var s = String(address).toLowerCase()
    for (var i = 0; i < s.length; i++) {
        h = (h ^ s.charCodeAt(i)) >>> 0
        h = Math.imul(h, 16777619) >>> 0
    }
    return ((h ^ (h >>> 13)) & 1) === 1
}

// Up to two initials: the first letters of the first two words ("GC"
// for Green Coast Trails), upper-cased; one word gives one letter.
function initials(name) {
    var plain = String(name).replace(/<[^>]*>/g, " ").replace(/"/g, "")
    var words = plain.split(/[\s.@_-]+/).filter(function(w) { return w.length > 0 })
    if (words.length === 0) return ""
    var first = function(w) { return Array.from(w)[0].toUpperCase() }
    return words.length === 1 ? first(words[0]) : first(words[0]) + first(words[1])
}

.pragma library

// Qt 5's Text.linkColor only reaches StyledText; in RichText an <a> keeps the
// palette link colour (#0000FF), and the sanitiser allows only href on it.
// So the colour goes in as a stylesheet in front of the body (#74).
// `color` is a "#rrggbb" string, e.g. String(theme.link).
// `lineHeight` (optional, a ratio such as theme.bodyLineHeight) goes in the
// same sheet: a TextEdit has no lineHeight property, and 1.0 adds nothing.
function withLinkColor(html, color, lineHeight) {
    let rules = "a{color:" + color + "}"
    const percent = Math.round((lineHeight || 1) * 100)
    if (percent !== 100) rules += "p,li,div{line-height:" + percent + "%}"
    return "<style>" + rules + "</style>" + html
}

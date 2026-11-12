.pragma library

// Qt 5's Text.linkColor only reaches StyledText; in RichText an <a> keeps the
// palette link colour (#0000FF), and the sanitiser allows only href on it.
// So the colour goes in as a stylesheet in front of the body (#74).
// `color` is a "#rrggbb" string, e.g. String(theme.link).
function withLinkColor(html, color) {
    return "<style>a{color:" + color + "}</style>" + html
}

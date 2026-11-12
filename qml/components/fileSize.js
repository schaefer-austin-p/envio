.pragma library

// A file's size for a list row, as on the boards: "812 B", "4.2 kB",
// "84 kB", "1.5 MB", "2.0 GB" (one decimal below 10, none above). QML's
// Locale has no formattedDataSize. Not a count of things, so it is not
// counted text.
function fileSize(bytes) {
    const num = function(n, digits) { return Number(n).toLocaleString(Qt.locale(), "f", digits) }
    if (bytes < 1000)
        // TRANSLATORS: %1 is a file size in bytes, like 812.
        return qsTr("%1 B").arg(num(bytes, 0))
    // One decimal below 10, none above; the unit is picked after rounding,
    // so 999 999 bytes is "1.0 MB", not "1,000 kB", and 9 999 is "10 kB".
    const shown = function(v) { return v < 10 ? Math.round(v * 10) / 10 : Math.round(v) }
    let value = bytes / 1000
    let unit = 1
    while (unit < 3 && shown(value) >= 1000) {
        value /= 1000
        unit++
    }
    const text = num(shown(value), shown(value) < 10 ? 1 : 0)
    if (unit === 1)
        // TRANSLATORS: %1 is a file size in kilobytes, like 4.2 or 84.
        return qsTr("%1 kB").arg(text)
    if (unit === 2)
        // TRANSLATORS: %1 is a file size in megabytes, like 1.5 or 106.
        return qsTr("%1 MB").arg(text)
    // TRANSLATORS: %1 is a file size in gigabytes, like 2.0.
    return qsTr("%1 GB").arg(text)
}

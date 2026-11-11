.pragma library

// A file's size for a list row, as on the boards: "812 B", "4.2 kB",
// "84 kB", "1.5 MB", "2.0 GB" (one decimal below 10, none above). QML's
// Locale has no formattedDataSize. Not a count of things, so it is not
// counted text.
function fileSize(bytes) {
    const num = function(n, digits) { return Number(n).toLocaleString(Qt.locale(), "f", digits) }
    const short = function(n) { return num(n, n < 10 ? 1 : 0) }
    if (bytes < 1000)
        // TRANSLATORS: %1 is a file size in bytes, like 812.
        return qsTr("%1 B").arg(num(bytes, 0))
    if (bytes < 1000 * 1000)
        // TRANSLATORS: %1 is a file size in kilobytes, like 4.2 or 84.
        return qsTr("%1 kB").arg(short(bytes / 1000))
    if (bytes < 1000 * 1000 * 1000)
        // TRANSLATORS: %1 is a file size in megabytes, like 1.5 or 106.
        return qsTr("%1 MB").arg(short(bytes / (1000 * 1000)))
    // TRANSLATORS: %1 is a file size in gigabytes, like 2.0.
    return qsTr("%1 GB").arg(short(bytes / (1000 * 1000 * 1000)))
}

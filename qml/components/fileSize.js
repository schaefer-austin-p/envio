.pragma library

// A file's size for a list row: "812 B", "4.2 KB", "1.5 MB". (QML's Locale
// has no formattedDataSize.) Not a count of things, so it is not counted text.
function fileSize(bytes) {
    const one = function(n) { return Number(n).toLocaleString(Qt.locale(), "f", 1) }
    if (bytes < 1024)
        //: TRANSLATORS: %1 is a file size in bytes, like 812.
        return qsTr("%1 B").arg(bytes)
    if (bytes < 1024 * 1024)
        //: TRANSLATORS: %1 is a file size in kilobytes, like 4.2.
        return qsTr("%1 KB").arg(one(bytes / 1024))
    //: TRANSLATORS: %1 is a file size in megabytes, like 1.5.
    return qsTr("%1 MB").arg(one(bytes / (1024 * 1024)))
}

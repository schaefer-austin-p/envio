import QtQuick 2.15
import "../platform"

// A message's date as the list shows it. Rust picks the bucket (`dateBucket`:
// "time", "weekday", "daymonth", "full" or "none"); this formats it with the
// user's locale. `date` is Unix seconds.
Text {
    textFormat: Text.PlainText
    id: root
    required property Theme theme
    property string bucket: "none"
    property real date: 0

    readonly property var when: new Date(date * 1000)
    text: {
        const l = Qt.locale()
        switch (bucket) {
        case "time": return when.toLocaleTimeString(l, l.timeFormat(Locale.ShortFormat))
        case "weekday": return l.dayName(when.getDay(), Locale.ShortFormat)
        case "daymonth": return when.toLocaleDateString(l, "d MMM")
        case "full": return when.toLocaleDateString(l, l.dateFormat(Locale.ShortFormat))
        default: return ""
        }
    }
    visible: text !== ""
    color: theme.secondary
    font.family: theme.fontMono
    font.pixelSize: 12
}

// expect: fontMono
import QtQuick 2.15

// Fixture: fontMono is banned outside qml/platform/Theme.qml; screens use
// Theme.fontAddress for hosts, links and the fingerprint, fontLabel for labels.
Text {
    required property var theme
    textFormat: Text.PlainText
    font.family: theme.fontMono
}

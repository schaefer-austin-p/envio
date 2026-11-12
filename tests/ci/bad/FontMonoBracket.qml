// expect: fontMono
import QtQuick 2.15

// Fixture: fontMono read by bracket is banned like the dotted form.
Text {
    required property var theme
    textFormat: Text.PlainText
    Component.onCompleted: font.family = theme["fontMono"]
}

// expect: Text.AutoText
import QtQuick 2.15

// Fixture: QtAutoText; must be rejected.
Text {
    text: "x"
    textFormat: Qt.AutoText
}

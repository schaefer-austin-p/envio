// expect: a count role in text
import QtQuick 2.15

// Fixture: a count given to .arg() in QML; must be rejected.
Text {
    property var model
    textFormat: Text.PlainText
    text: qsTr("Inbox %1").arg(model.unread)
}

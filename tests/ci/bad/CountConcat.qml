// expect: a count role in text
import QtQuick 2.15

// Fixture: a count joined into text in QML; must be rejected.
Text {
    property var model
    textFormat: Text.PlainText
    text: model.name + "  " + model.unread
}

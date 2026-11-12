// expect: template string
import QtQuick 2.15

// Fixture: a template string hides its ${} code from the lint; must be rejected.
Text {
    property int n: 2
    textFormat: Text.PlainText
    text: `${qsTr("%n messages", "", n)}`
}

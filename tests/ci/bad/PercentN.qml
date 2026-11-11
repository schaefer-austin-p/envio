import QtQuick 2.15

// Fixture: %n inside a translated string; must be rejected.
Text {
    text: qsTr("%n selected")
}

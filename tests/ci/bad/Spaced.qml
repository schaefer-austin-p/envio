// expect: qsTr with a count argument
import QtQuick 2.15

// Fixture: a space before the parenthesis; must be rejected.
Text {
    text: qsTr ("selected", "", 3)
}

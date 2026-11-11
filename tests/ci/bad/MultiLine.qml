import QtQuick 2.15

// Fixture: the count is on a later line; must be rejected.
Text {
    property int count: 2
    text: qsTr("selected",
               "",
               count)
}

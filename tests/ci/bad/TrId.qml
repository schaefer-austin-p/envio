import QtQuick 2.15

// Fixture: qsTrId with a count; must be rejected.
Text {
    text: qsTrId("id_selected", 3)
}

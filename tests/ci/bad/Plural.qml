// expect: qsTr with a count argument
import QtQuick 2.15

// Fixture for tools/ci/no-qml-plurals.sh: must be rejected.
Text {
    property int n: 2
    text: qsTr("%n messages selected", "", n)
}

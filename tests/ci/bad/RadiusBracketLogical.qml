// expect: radius '&&= 4'
import QtQuick 2.15

// Fixture: a logical assignment through a bracket key fails too.
Rectangle {
    required property var theme
    Component.onCompleted: item["radius"] &&= 4
}

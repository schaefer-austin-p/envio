// expect: radius '||= 4'
import QtQuick 2.15

// Fixture: `||=` fails like any compound assignment.
Rectangle {
    required property var theme
    Component.onCompleted: radius ||= 4
}

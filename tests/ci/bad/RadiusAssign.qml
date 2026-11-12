// expect: radius '4'
import QtQuick 2.15

// Fixture: a JS assignment is checked like a binding.
Rectangle {
    required property var theme
    Component.onCompleted: radius = 4
}

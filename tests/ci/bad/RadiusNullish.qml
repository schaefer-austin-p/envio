// expect: radius '??= 4'
import QtQuick 2.15

// Fixture: a logical assignment (??=, ||=, &&=) fails like any compound one.
Rectangle {
    required property var theme
    Component.onCompleted: radius ??= 4
}

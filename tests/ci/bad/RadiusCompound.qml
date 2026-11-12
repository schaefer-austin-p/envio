// expect: radius '+= 4'
import QtQuick 2.15

// Fixture: a compound assignment fails.
Rectangle {
    required property var theme
    Component.onCompleted: radius += 4
}

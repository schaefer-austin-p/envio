// expect: radius '4'
import QtQuick 2.15

// Fixture: a bracket assignment fails.
Rectangle {
    required property var theme
    Component.onCompleted: item["radius"] = 4
}

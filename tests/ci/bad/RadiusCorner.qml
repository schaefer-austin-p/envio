// expect: radius '6'
import QtQuick 2.15

// Fixture: a per-corner radius is no exception either.
Rectangle {
    required property var theme
    topLeftRadius: 6
}

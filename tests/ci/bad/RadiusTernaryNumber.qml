// expect: radius 'cond ? theme.cardRadius : 4'
import QtQuick 2.15

// Fixture: a ternary with a numeric branch fails.
Rectangle {
    required property var theme
    radius: cond ? theme.cardRadius : 4
}

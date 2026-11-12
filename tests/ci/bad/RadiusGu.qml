// expect: radius 'theme.gu(1)'
import QtQuick 2.15

// Fixture: a gu() size is no role either.
Rectangle {
    required property var theme
    radius: theme.gu(1)
}

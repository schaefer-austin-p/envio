// expect: radius 'theme.cardRadius + 2'
import QtQuick 2.15

// Fixture: a role plus a number fails.
Rectangle {
    required property var theme
    radius: theme.cardRadius + 2
}

// expect: radius '8'
import QtQuick 2.15

// Fixture: a corner radius is a Theme role (controlRadius, cardRadius, ...),
// never a number in a component or page.
Rectangle {
    required property var theme
    radius: 8
}

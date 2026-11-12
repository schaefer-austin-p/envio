// expect: radius '5'
import QtQuick 2.15

// Fixture: a Binding on a radius takes a role too.
Rectangle {
    id: box
    required property var theme
    Binding { target: box; property: "radius"; value: 5 }
}

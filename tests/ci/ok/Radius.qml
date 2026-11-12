import QtQuick 2.15

// Fixture: Theme roles and circle forms pass the radius rule.
Item {
    required property var theme
    Rectangle { radius: theme.controlRadius }
    Rectangle { width: 10; height: width; radius: width / 2 }
    Rectangle { radius: height/2 }
    Rectangle { radius: cond ? theme.checkRadius : width / 2 }
}

import QtQuick 2.15

// Fixture: Theme roles and circle forms pass the radius rule.
Item {
    required property var theme
    Rectangle { radius: theme.controlRadius }
    Rectangle { width: 10; height: width; radius: width / 2 }
    Rectangle { radius: height/2 }
    Rectangle { radius: cond ? theme.checkRadius : width / 2 }
    Rectangle { radius: theme.avatarRadius(size) }
    Rectangle { radius: Math.min(width, height) / 2 }
    Rectangle { radius: parent.width / 2 }
    Rectangle { radius: width / 2 - 1 }
    // A JS local named radius is no radius property.
    function f() { const radius = 4; let r2 = radius; var radius = 3; return r2 }
}

import QtQuick 2.15
import "../../platform"
import "../../components"

// Remote images are blocked (board 3): what that means, and the two ways to
// load them, for this message or always for its sender.
Column {
    id: root
    required property Theme theme
    signal loadRequested()
    signal alwaysRequested()

    Banner {
        theme: root.theme
        width: parent.width
        text: qsTr("Remote images are blocked. They can tell the sender when and where you opened this.")
    }
    Rectangle {
        width: parent.width
        height: buttons.height + root.theme.gu(2)
        color: root.theme.selected
        Flow {
            id: buttons
            x: root.theme.gu(2)
            y: root.theme.gu(0.5)
            width: parent.width - root.theme.gu(4)
            spacing: root.theme.gu(1)
            AppButton {
                objectName: "loadImages"
                theme: root.theme
                primary: true
                text: qsTr("Load images")
                onClicked: root.loadRequested()
            }
            AppButton {
                objectName: "alwaysForSender"
                theme: root.theme
                text: qsTr("Always for this sender")
                onClicked: root.alwaysRequested()
            }
        }
    }
}

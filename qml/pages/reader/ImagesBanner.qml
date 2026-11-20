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
    // The server vouched for the From address; only then does "always for
    // this sender" do anything (the core ignores the allow otherwise).
    property bool senderVerified: false
    // The message carries a DKIM signature that could verify it. Mail from a
    // sender who is not allowed yet is never looked up, so it cannot be
    // verified in advance; the button shows for it anyway (decision 8, owner
    // decision O3, #285). Safety does not depend on the button: the allow
    // never applies to a later message that does not verify.
    property bool senderCheckable: false

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
                visible: root.senderVerified || root.senderCheckable
                theme: root.theme
                text: qsTr("Always for this sender")
                onClicked: root.alwaysRequested()
            }
        }
    }
}

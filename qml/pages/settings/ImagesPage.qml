import QtQuick 2.15
import "../../platform"
import "../../components"

// Load remote images (board 13b): ask each time or always, and the
// senders whose images always load, each removable.
Item {
    id: root
    objectName: "imagesPage"
    required property Theme theme
    required property var core
    signal backRequested()

    function save(always) {
        core.saveSettings(core.theme, core.darkMode, always, core.defaultAccount)
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Load remote images")
        onBackClicked: root.backRequested()
    }

    Flickable {
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        clip: true
        contentHeight: col.height + root.theme.gu(2)
        Column {
            id: col
            width: parent.width
            RadioRow {
                objectName: "images-ask"
                theme: root.theme
                width: parent.width
                text: qsTr("Ask each time")
                caption: qsTr("Mail shows without pictures until you load them.")
                checked: !root.core.imagesAlways
                onActivated: root.save(false)
            }
            RadioRow {
                objectName: "images-always"
                theme: root.theme
                width: parent.width
                text: qsTr("Always")
                caption: qsTr("Pictures in every message load at once. The sender can tell you opened it.")
                checked: root.core.imagesAlways
                onActivated: root.save(true)
            }
            Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2)
                topPadding: root.theme.gu(2)
                width: parent.width - root.theme.gu(4)
                text: qsTr("Always load from")
                color: root.theme.secondary
                font.family: root.theme.fontMono
                font.pixelSize: 11
                font.letterSpacing: 0.6
                font.capitalization: Font.AllUppercase
            }
            Text {
                objectName: "noSenders"
                textFormat: Text.PlainText
                x: root.theme.gu(2)
                width: parent.width - root.theme.gu(4)
                visible: root.core.allowedSenders.length === 0
                height: visible ? implicitHeight + root.theme.gu(1) : 0
                verticalAlignment: Text.AlignVCenter
                text: qsTr("No senders yet. Choose \"Always load from this sender\" in a message.")
                wrapMode: Text.WordWrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 14
            }
            Repeater {
                model: root.core.allowedSenders
                delegate: Item {
                    id: sender
                    required property string modelData
                    width: col.width
                    height: root.theme.touch + root.theme.gu(1)
                    Text {
                        textFormat: Text.PlainText
                        anchors { left: parent.left; leftMargin: root.theme.gu(2); right: removeButton.left; rightMargin: root.theme.gu(1); verticalCenter: parent.verticalCenter }
                        text: sender.modelData
                        elide: Text.ElideMiddle
                        color: root.theme.text
                        font.family: root.theme.fontUi
                        font.pixelSize: 15
                    }
                    AppButton {
                        id: removeButton
                        objectName: "remove-" + sender.modelData
                        theme: root.theme
                        anchors { right: parent.right; rightMargin: root.theme.gu(2); verticalCenter: parent.verticalCenter }
                        text: qsTr("Remove")
                        onClicked: root.core.disallowImagesFrom(sender.modelData)
                    }
                }
            }
        }
    }
}

import QtQuick 2.15
import "../../platform"
import "../../components"

// Sync (board 13c): per account, what "checking" means (new mail is
// fetched while Envio is open) and how long mail stays on this phone. The
// 30 days are fixed in 0.1.0, so that row only tells.
Item {
    id: root
    objectName: "syncPage"
    required property Theme theme
    required property var accounts
    signal backRequested()

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Sync")
        onBackClicked: root.backRequested()
    }

    Flickable {
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        clip: true
        contentHeight: col.height + root.theme.gu(2)
        Column {
            id: col
            width: parent.width
            Repeater {
                model: root.accounts
                delegate: Column {
                    id: acct
                    required property var model
                    width: col.width
                    Text {
                        textFormat: Text.PlainText
                        x: root.theme.gu(2)
                        topPadding: root.theme.gu(2)
                        width: parent.width - root.theme.gu(4)
                        text: acct.model.name
                        elide: Text.ElideRight
                        color: root.theme.heading
                        font.family: root.theme.fontUi
                        font.pixelSize: 16
                        font.weight: Font.Medium
                    }
                    RadioRow {
                        objectName: "whileOpen-" + acct.model.key
                        theme: root.theme
                        width: parent.width
                        text: qsTr("While Envio is open")
                        caption: qsTr("New mail arrives while the app is open. When it is closed, Envio does not check.")
                        checked: true
                    }
                    Item {
                        width: parent.width
                        height: root.theme.touch
                        Text {
                            textFormat: Text.PlainText
                            anchors { left: parent.left; leftMargin: root.theme.gu(2); verticalCenter: parent.verticalCenter }
                            text: qsTr("Keep mail on this phone for")
                            color: root.theme.text
                            font.family: root.theme.fontUi
                            font.pixelSize: 16
                        }
                        Text {
                            objectName: "keepFor-" + acct.model.key
                            textFormat: Text.PlainText
                            anchors { right: parent.right; rightMargin: root.theme.gu(2); verticalCenter: parent.verticalCenter }
                            text: qsTr("Last 30 days")
                            color: root.theme.secondary
                            font.family: root.theme.fontUi
                            font.pixelSize: 15
                        }
                    }
                }
            }
        }
    }
}

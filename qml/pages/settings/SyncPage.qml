import QtQuick 2.15
import "../../platform"
import "../../components"

// Sync (board 13c): how long mail stays on this phone (one choice for all
// accounts) and, per account, what "checking" means (new mail is fetched
// while Envio is open).
Item {
    id: root
    objectName: "syncPage"
    required property Theme theme
    required property var accounts
    required property var core
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
            Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2)
                topPadding: root.theme.gu(2)
                width: parent.width - root.theme.gu(4)
                text: qsTr("Keep mail on this phone for")
                color: root.theme.heading
                font.family: root.theme.fontUi
                font.pixelSize: 16
                font.weight: Font.Medium
            }
            Repeater {
                model: [
                    { key: "30", days: 30, label: qsTr("Last 30 days") },
                    { key: "90", days: 90, label: qsTr("Last 90 days") },
                    { key: "365", days: 365, label: qsTr("Last year") },
                    { key: "all", days: 0, label: qsTr("All mail") }
                ]
                delegate: RadioRow {
                    required property var modelData
                    objectName: "window-" + modelData.key
                    theme: root.theme
                    width: col.width
                    text: modelData.label
                    selected: root.core.syncDays === modelData.days
                    onActivated: root.core.setSyncWindow(modelData.days)
                }
            }
            Repeater {
                model: root.accounts
                delegate: Column {
                    id: acct
                    required property var model
                    width: col.width
                    Text {
                        objectName: "syncAccount-" + acct.model.key
                        textFormat: Text.PlainText
                        x: root.theme.gu(2)
                        topPadding: root.theme.gu(2)
                        width: parent.width - root.theme.gu(4)
                        text: acct.model.display
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
                        selected: true
                    }
                }
            }
        }
    }
}

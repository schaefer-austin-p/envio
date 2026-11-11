import QtQuick 2.15
import Envio 1.0
import "../../platform"
import "../../components"

// About: the version and the commit this build was made from.
Item {
    id: root
    objectName: "aboutPage"
    required property Theme theme
    signal backRequested()

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("About")
        onBackClicked: root.backRequested()
    }

    Column {
        anchors { top: bar.bottom; left: parent.left; right: parent.right; margins: root.theme.gu(2.5) }
        anchors.topMargin: root.theme.gu(3)
        spacing: root.theme.gu(1)
        Text {
            textFormat: Text.PlainText
            text: "Envio"
            color: root.theme.heading
            font.family: root.theme.fontTitle
            font.pixelSize: 24
            font.weight: Font.DemiBold
        }
        Text {
            objectName: "version"
            textFormat: Text.PlainText
            text: BuildInfo.version
            color: root.theme.text
            font.family: root.theme.fontMono
            font.pixelSize: 15
        }
        Text {
            objectName: "commit"
            textFormat: Text.PlainText
            text: BuildInfo.commit
            color: root.theme.secondary
            font.family: root.theme.fontMono
            font.pixelSize: 13
        }
    }
}

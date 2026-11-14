import QtQuick 2.15
import Envio 1.0
import "../../platform"
import "../../components"

// About: the version, and for a development build what it was made from.
Item {
    id: root
    objectName: "aboutPage"
    required property Theme theme
    signal backRequested()

    // BuildInfo's times are milliseconds since 1970, 0 when not known.
    function when(ms) {
        return ms > 0 ? new Date(ms).toLocaleString(Qt.locale(), Locale.ShortFormat) : qsTr("unknown")
    }

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
            font.family: root.theme.fontLabel
            font.pixelSize: 15
        }
        // A development build says what it was made from and when (#198);
        // an official release shows only the version.
        Text {
            objectName: "build"
            visible: BuildInfo.dev
            width: parent.width
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: [
                qsTr("Development build"),
                BuildInfo.source,
                qsTr("Built %1").arg(root.when(BuildInfo.built)),
                qsTr("Installed %1").arg(BuildInfo.installed > 0 ? root.when(BuildInfo.installed)
                                                                  : qsTr("not as a click"))
            ].join("\n")
            color: root.theme.secondary
            font.family: root.theme.fontLabel
            font.pixelSize: 13
            lineHeight: 1.3
        }
    }
}

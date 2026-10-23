import QtQuick 2.7
import QtQuick.Controls 2.2
import Lomiri.Components 1.3

ApplicationWindow {
    id: root
    objectName: 'mainView'

    width: units.gu(45)
    height: units.gu(75)
    visible: true
    // The QtQuick.Controls Suru style ignores the system theme; Lomiri's follows it.
    color: theme.palette.normal.background

    Label {
        anchors.centerIn: parent
        text: i18n.tr("Hello world!")
    }
}
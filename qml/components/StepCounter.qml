import QtQuick 2.15
import "../platform"

// "Step 2 of 3": a small label under the top bar (Decision 8). `total` is 0
// while the path is not known yet (the provider step): "Step 1" alone.
Item {
    id: root
    required property Theme theme
    property int step: 1
    property int total: 0
    implicitHeight: label.implicitHeight + theme.gu(1.5)
    height: implicitHeight

    Text {
        id: label
        objectName: "stepCounterText"
        textFormat: Text.PlainText
        x: root.theme.gu(2.5)
        y: root.theme.gu(0.5)
        text: root.total > 0 ? qsTr("Step %1 of %2").arg(root.step).arg(root.total)
                             : qsTr("Step %1").arg(root.step)
        // The section-label look of the theme (11 px in Carbon).
        font.capitalization: root.theme.eyebrowCaps ? Font.AllUppercase : Font.MixedCase
        font.letterSpacing: root.theme.eyebrowSpacing
        font.family: root.theme.eyebrowFamily
        font.pixelSize: root.theme.eyebrowSize
        font.weight: root.theme.eyebrowWeight
        color: root.theme.secondary
    }
}

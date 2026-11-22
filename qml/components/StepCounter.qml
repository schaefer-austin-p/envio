import QtQuick 2.15
import "../platform"

// "Step 2 of 3": a small label under the top bar (Decision 8), and below it a
// bar of `total` segments, the first `step` filled (the boards' step bar).
// `total` is 0 while the path is not known yet (the provider step): "Step 1"
// alone, no bar. The bar stays here: bindle has no Controls tier yet, and
// Platform and Behaviours draw nothing.
Item {
    id: root
    required property Theme theme
    property int step: 1
    property int total: 0
    implicitHeight: label.implicitHeight + theme.gu(1.5)
                    + (segments.visible ? theme.gu(1.5) : 0)
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

    Row {
        id: segments
        objectName: "stepSegments"
        readonly property int count: root.total
        readonly property int filled: root.step
        visible: count > 0
        x: root.theme.gu(2.5)
        y: label.y + label.implicitHeight + root.theme.gu(1)
        width: root.width - root.theme.gu(5)
        spacing: root.theme.gu(0.75)
        Repeater {
            model: segments.count
            Rectangle {
                objectName: "stepSegment"
                width: (segments.width - (segments.count - 1) * segments.spacing) / segments.count
                height: root.theme.gu(0.5)
                radius: height / 2
                color: index < segments.filled ? root.theme.primary : root.theme.hairline
            }
        }
    }
}

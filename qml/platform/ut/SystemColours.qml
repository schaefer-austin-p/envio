import QtQuick 2.15
import QtQuick.Controls.Suru 2.2

// Ubuntu Touch's window colour from the Suru style, whose palette stays
// light when the system theme is dark. Suru lives only here.
Item {
    readonly property color windowColour: Suru.backgroundColor
}

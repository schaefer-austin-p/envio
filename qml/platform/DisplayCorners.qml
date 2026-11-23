import QtQuick 2.15

// Whether the window's frame corners meet rounded display corners (#348): the
// platform rounds them only in a Lomiri session (`phoneSession`, from
// `lomiriSession` like WindowMode), and only a window that spans the screen's
// width is at them. A staged phone window always spans the width; only its
// height loses the indicator panel, whose height does not follow gu, so height
// is no reliable signal and is not tested. A windowed convergence window is
// narrower. `widthTolerance` absorbs a pixel of rounding. Compared by
// geometry, not Window.visibility (the compositor may report Maximized for a
// windowed app; sizes are injectable and follow a resize by binding). Main
// feeds the window and Screen widths and binds the result to
// Theme.roundedDisplay.
Item {
    visible: false

    property bool phoneSession: typeof lomiriSession !== "undefined" && lomiriSession === true
    property real windowWidth: 0
    property real windowHeight: 0
    property real screenWidth: 0
    property real screenHeight: 0
    // Pixels of slack on the width comparison.
    property real widthTolerance: 2

    readonly property bool rounded: phoneSession && screenWidth > 0 && screenHeight > 0
        && windowWidth >= screenWidth - widthTolerance
}

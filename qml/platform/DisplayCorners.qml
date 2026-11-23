import QtQuick 2.15

// Whether the window's frame corners meet rounded display corners (#348): the
// platform rounds them only in a Lomiri session (`phoneSession`, from
// `lomiriSession` like WindowMode), and only a window that covers the whole
// screen reaches them; a windowed or convergence-mode window is smaller and
// its corners are square. Compared by geometry, not Window.visibility: the
// sizes are what decides whether the corner is on screen, whatever state the
// compositor reports, and they follow a resize by binding. Main feeds the
// window and Screen sizes and binds the result to Theme.roundedDisplay; the
// tests set every input.
Item {
    visible: false

    property bool phoneSession: typeof lomiriSession !== "undefined" && lomiriSession === true
    property real windowWidth: 0
    property real windowHeight: 0
    property real screenWidth: 0
    property real screenHeight: 0

    readonly property bool rounded: phoneSession && screenWidth > 0 && screenHeight > 0
        && windowWidth >= screenWidth && windowHeight >= screenHeight
}

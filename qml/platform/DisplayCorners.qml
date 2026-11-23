import QtQuick 2.15

// Whether the window's frame corners meet rounded display corners (#348): the
// platform rounds them only in a Lomiri session (`phoneSession`, from
// `lomiriSession` like WindowMode), and only a window that reaches the
// screen's sides and bottom is at them. On the phone the indicator panel sits
// above the app, so a normal (staged) window is `panelAllowance` or so shorter
// than the screen and still counts; a convergence window is narrower than the
// screen, so the width test is what turns it square. Compared by geometry, not
// Window.visibility (the compositor may report Maximized for a windowed app;
// sizes are injectable and follow a resize by binding). Main feeds the sizes
// and gu(6) and binds the result to Theme.roundedDisplay.
Item {
    visible: false

    property bool phoneSession: typeof lomiriSession !== "undefined" && lomiriSession === true
    property real windowWidth: 0
    property real windowHeight: 0
    property real screenWidth: 0
    property real screenHeight: 0
    // The most the indicator panel takes off the height: gu(6) from Main.
    property real panelAllowance: 48

    readonly property bool rounded: phoneSession && screenWidth > 0 && screenHeight > 0
        && windowWidth >= screenWidth && windowHeight >= screenHeight - panelAllowance
}

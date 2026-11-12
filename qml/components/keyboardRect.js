.pragma library

// The on-screen keyboard's rectangle in window logical coordinates.
// Ubuntu Touch reports Qt.inputMethod.keyboardRectangle in device pixels
// (x 0, y 1470, w 1080, h 822 on a 1080x2292 screen at 2.625), while items
// live in logical ones, so it is divided by the device pixel ratio. A rect
// no wider than the screen in logical pixels (desktop, or any platform that
// already reports logical pixels) is used as it is.
function logical(rect, screenWidth, dpr) {
    if (!(dpr > 0) || rect.width <= screenWidth + 1)
        return rect
    return Qt.rect(rect.x / dpr, rect.y / dpr, rect.width / dpr, rect.height / dpr)
}

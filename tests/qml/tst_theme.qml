import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"

TestCase {
    name: "Theme"

    Theme { id: theme }

    function init() {
        theme.airmail = false
        theme.darkOverride = null
    }

    function test_carbon_light() {
        theme.darkOverride = false
        compare(theme.dark, false)
        compare(String(theme.ground), "#f8f6f1")
        compare(String(theme.text), "#1e2a38")
        compare(String(theme.accent), "#b8342c")
    }

    function test_carbon_dark() {
        theme.darkOverride = true
        compare(theme.dark, true)
        compare(String(theme.ground), "#15181d")
        compare(String(theme.accent), "#e36a60")
        compare(String(theme.primaryText), "#15181d")
    }

    function test_airmail_light_has_its_own_heading() {
        theme.airmail = true
        theme.darkOverride = false
        compare(String(theme.ground), "#fbf8f1")
        compare(String(theme.heading), "#1f3a5f")
    }

    function test_airmail_dark() {
        theme.airmail = true
        theme.darkOverride = true
        compare(String(theme.ground), "#141c27")
        compare(String(theme.heading), "#a9c1e3")
    }

    function test_follows_system_when_not_forced() {
        compare(theme.dark, theme.systemWindow.hslLightness < 0.5)
    }

    // WCAG relative luminance and contrast ratio.
    function luminance(c) {
        function channel(v) { return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4) }
        return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
    }
    function contrast(a, b) {
        const la = luminance(a), lb = luminance(b)
        return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
    }

    function test_text_roles_meet_aa_data() {
        return [
            { tag: "carbon light", airmail: false, dark: false },
            { tag: "carbon dark", airmail: false, dark: true },
            { tag: "airmail light", airmail: true, dark: false },
            { tag: "airmail dark", airmail: true, dark: true }
        ]
    }

    // docs/10-design.md: text, secondary and accent on ground reach 4.5:1.
    // A failing value is changed in the doc first, then here and in Theme.qml.
    function test_text_roles_meet_aa(data) {
        theme.airmail = data.airmail
        theme.darkOverride = data.dark
        for (const role of ["text", "heading", "secondary", "accent"]) {
            const ratio = contrast(theme[role], theme.ground)
            verify(ratio >= 4.5, role + " on ground is " + ratio.toFixed(2) + ":1")
        }
    }
}

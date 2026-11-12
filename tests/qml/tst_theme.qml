import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"

TestCase {
    name: "Theme"

    Theme { id: theme }

    function init() {
        theme.name = "carbon"
        theme.darkMode = "system"
    }

    function test_carbon_light() {
        theme.darkMode = "light"
        compare(theme.dark, false)
        compare(String(theme.ground), "#f8f6f1")
        compare(String(theme.text), "#1e2a38")
        compare(String(theme.accent), "#b8342c")
    }

    function test_carbon_dark() {
        theme.darkMode = "dark"
        compare(theme.dark, true)
        compare(String(theme.ground), "#15181d")
        compare(String(theme.accent), "#e36a60")
        compare(String(theme.primaryText), "#15181d")
    }

    function test_airmail_light_has_its_own_heading() {
        theme.name = "airmail"
        theme.darkMode = "light"
        compare(String(theme.ground), "#fbf8f1")
        compare(String(theme.heading), "#1f3a5f")
    }

    function test_airmail_dark() {
        theme.name = "airmail"
        theme.darkMode = "dark"
        compare(String(theme.ground), "#141c27")
        compare(String(theme.heading), "#a9c1e3")
    }

    function test_system_dark_flips_roles() {
        theme.darkMode = "system"
        theme.systemDark = false
        compare(String(theme.ground), "#f8f6f1")
        theme.systemDark = true
        compare(theme.dark, true)
        compare(String(theme.ground), "#15181d")
        compare(String(theme.text), "#e8e5de")
    }

    function test_forced_modes_ignore_system() {
        theme.systemDark = true
        theme.darkMode = "light"
        compare(String(theme.ground), "#f8f6f1")
        theme.systemDark = false
        theme.darkMode = "dark"
        compare(String(theme.ground), "#15181d")
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
        theme.name = data.airmail ? "airmail" : "carbon"
        theme.darkMode = data.dark ? "dark" : "light"
        for (const role of ["text", "heading", "secondary", "accent"]) {
            const ratio = contrast(theme[role], theme.ground)
            verify(ratio >= 4.5, role + " on ground is " + ratio.toFixed(2) + ":1")
        }
    }

    function test_scrim_and_chevron_exist_everywhere(data) {
        theme.name = data.airmail ? "airmail" : "carbon"
        theme.darkMode = data.dark ? "dark" : "light"
        verify(theme.scrim.a > 0.4 && theme.scrim.a < 0.6, "scrim alpha " + theme.scrim.a)
        verify(theme.chevron.a === 1 && theme.chevronBlue.a === 1)
    }
    function test_scrim_and_chevron_exist_everywhere_data() { return test_text_roles_meet_aa_data() }

    function test_carbon_light_scrim_is_the_designed_one() {
        theme.darkMode = "light"
        compare(Math.round(theme.scrim.r * 255), 30)
        compare(Math.round(theme.scrim.g * 255), 42)
        compare(Math.round(theme.scrim.b * 255), 56)
        fuzzyCompare(theme.scrim.a, 0.45, 0.005)
    }

    // #5: the label of a primary button, on the button.
    function test_primary_text_on_primary_contrast(data) {
        theme.name = data.airmail ? "airmail" : "carbon"
        theme.darkMode = data.dark ? "dark" : "light"
        const ratio = contrast(theme.primaryText, theme.primary)
        verify(ratio >= 4.5, "primaryText on primary is " + ratio.toFixed(2) + ":1")
    }
    function test_primary_text_on_primary_contrast_data() { return test_text_roles_meet_aa_data() }

    function test_chevron_contrast(data) {
        theme.name = data.airmail ? "airmail" : "carbon"
        theme.darkMode = data.dark ? "dark" : "light"
        for (const role of ["chevron", "chevronBlue"]) {
            const ratio = contrast(theme[role], theme.ground)
            verify(ratio >= 3, role + " on ground is " + ratio.toFixed(2) + ":1")
        }
    }
    function test_chevron_contrast_data() { return test_text_roles_meet_aa_data() }

    function test_fonts_and_grid() {
        compare(theme.fontUi, "Inter")
        compare(theme.fontMono, "IBM Plex Mono")
        theme.name = "carbon"
        compare(theme.fontTitle, theme.fontMono)
        theme.name = "airmail"
        compare(theme.fontTitle, "Lora")
        compare(theme.gu(2), theme.gridUnit * 2)
        verify(theme.touch >= 44)
    }

    // #67: label, body and eyebrow roles.
    function test_font_roles_in_carbon() {
        theme.name = "carbon"
        compare(theme.fontLabel, theme.fontMono)
        compare(theme.fontBody, theme.fontUi)
        compare(theme.bodySize, 16)
        compare(theme.bodyLineHeight, 1.0)
        compare(theme.eyebrowFamily, theme.fontMono)
        compare(theme.eyebrowSize, 11)
        compare(theme.eyebrowWeight, Font.Normal)
        compare(theme.eyebrowCaps, true)
        fuzzyCompare(theme.eyebrowSpacing, 0.6, 0.001)
    }

    function test_font_roles_in_airmail() {
        theme.name = "airmail"
        verify(theme.fontLabel !== theme.fontMono)
        compare(theme.fontLabel, theme.fontUi)
        compare(theme.fontBody, "Lora")
        compare(theme.bodySize, 16)
        fuzzyCompare(theme.bodyLineHeight, 1.6, 0.001)
        compare(theme.eyebrowFamily, theme.fontUi)
        compare(theme.eyebrowSize, 12)
        compare(theme.eyebrowWeight, Font.DemiBold)
        compare(theme.eyebrowCaps, true)
        fuzzyCompare(theme.eyebrowSpacing, 1.2, 0.001)
    }
}

.pragma library

// The wizard's path, as plain functions of its state: which step number it
// is, and what a refused sign-in says. The path is fixed once the provider is
// picked: provider, sign-in, options (3), or with Other provider, email,
// sign-in, options (4).

function isMicrosoft(provider) {
    return provider === "outlook" || provider === "m365"
}
// Servers and the address step only exist on the Other path.
function isOther(step, provider) {
    return provider === "other" || step === "email" || step === "servers"
}
function stepNumber(step, provider) {
    if (step === "provider") return 1
    if (step === "options") return isOther(step, provider) ? 4 : 3
    if (step === "email") return 2
    return isOther(step, provider) ? 3 : 2
}
// 0 on the provider step: the path is not known yet.
function stepTotal(step, provider) {
    if (step === "provider") return 0
    return isOther(step, provider) ? 4 : 3
}
function providerName(provider) {
    return provider === "outlook" ? "Outlook.com" : "Microsoft 365"
}

// The card for a sign-in that ended with nothing saved: {kind, title, text}.
// `text` is the core's own sentence, used when the outcome is not "admin" or
// "cancelled".
function refusal(outcome, text) {
    if (outcome === "cancelled")
        return { "kind": "info", "title": qsTr("Sign-in cancelled"),
                 "text": qsTr("Nothing was saved. Choose a provider to try again.") }
    if (outcome === "admin")
        return { "kind": "warning", "title": qsTr("Your organization has not allowed Envio"),
                 "text": qsTr("Ask your administrator to allow it. Nothing was saved.") }
    return { "kind": "warning", "title": qsTr("Nothing was saved"), "text": text }
}

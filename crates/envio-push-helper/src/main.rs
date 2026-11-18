//! `envio-push-helper <in> <out>`: the push service starts it for every
//! notification, under its own AppArmor profile. It reads one file, writes
//! one, and never waits on anything (the service allows it 5 s).

mod card;
mod text;

use std::{env, fs, path::PathBuf, process::ExitCode};

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let [_, input, output] = args.as_slice() else {
        eprintln!("usage: envio-push-helper <in> <out>");
        return ExitCode::from(2);
    };
    translations();
    // Unreadable input is bad input: the service still gets an answer.
    let data = fs::read(input).unwrap_or_default();
    let out = card::convert(&data, &text::generic());
    match fs::write(output, out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("envio-push-helper: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The catalogues ship in the click's `share/locale`, one level above
/// `push/`. The profile may not let the helper read them: the text then
/// stays English.
fn translations() {
    setlocale(LocaleCategory::LcAll, "");
    let locale: Option<PathBuf> = env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("share/locale")));
    if let Some(dir) = locale.filter(|dir| dir.is_dir()) {
        let _ = bindtextdomain("envio.aps", dir);
    }
    // The card JSON is UTF-8, whatever the locale's charset (as qt/main.cpp).
    let _ = bind_textdomain_codeset("envio.aps", "UTF-8");
}

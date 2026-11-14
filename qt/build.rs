use cxx_qt_build::{CxxQtBuilder, QmlModule};

// Which build this is comes from envio-stamp (qt/stamp), so a new commit
// recompiles that crate and this one's Rust, not the generated C++.
fn main() {
    println!("cargo:rerun-if-changed=../manifest.json");
    CxxQtBuilder::new_qml_module(QmlModule::new("Envio").version(1, 0))
        .files(["src/build_info.rs", "src/core_object.rs", "src/models.rs"])
        .build();
}

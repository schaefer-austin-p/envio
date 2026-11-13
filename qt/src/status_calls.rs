//! The status invokables of `EnvioCore`: text the core words, QML shows.

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use envio_ui::BannerTab;

use crate::core_object::qobject;

impl qobject::EnvioCore {
    /// The offline banner text of a tab (`envio_ui::Ui::offline_banner`).
    pub fn offline_text(&self, tab: &QString, folder: i32, waiting: i32) -> QString {
        let tab = match String::from(tab).as_str() {
            "" => BannerTab::Unified,
            "folder" => BannerTab::Folder(i64::from(folder)),
            key => BannerTab::Account(key.to_owned()),
        };
        let waiting = u32::try_from(waiting).unwrap_or(0);
        QString::from(self.rust().ui.offline_banner(&tab, waiting))
    }
}

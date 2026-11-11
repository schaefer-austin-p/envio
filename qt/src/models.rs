//! `RowsModel`: one `QAbstractListModel` for all six lists. It holds the rows
//! envio-ui produced and applies envio-ui's `Op`s with the matching
//! begin/end calls; role names and values come from envio-ui.

// Qt's begin/end calls fix the arguments (beginInsertRows takes four,
// plus self); the argument-count gate does not fit the bridge.
#![allow(clippy::too_many_arguments)]

use envio_ui::{Model, Row, RowValue};

/// The first role id; role `i` of the model's `role_names` is `FIRST_ROLE + i`
/// (Qt::UserRole is 0x100).
const FIRST_ROLE: i32 = 0x100;

/// "accounts", "folders", "messages", "outbox", "search" or "suggestions".
pub fn model_of(kind: &str) -> Option<Model> {
    Some(match kind {
        "accounts" => Model::Accounts,
        "folders" => Model::Folders,
        "messages" => Model::Messages,
        "outbox" => Model::Outbox,
        "search" => Model::Search,
        "suggestions" => Model::Suggestions,
        _ => return None,
    })
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, kind)]
        type RowsModel = super::RowsModelRust;
    }

    impl cxx_qt::Threading for RowsModel {}

    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut RowsModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
    }

    extern "RustQt" {
        /// # Safety
        ///
        /// Every begin call is followed by its end call.
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(
            self: Pin<&mut RowsModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut RowsModel>);

        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut RowsModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut RowsModel>);

        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut RowsModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut RowsModel>);
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn index(self: &RowsModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &RowsModel, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &RowsModel) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &RowsModel, parent: &QModelIndex) -> i32;
    }
}

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};
use envio_ui::Op;

#[derive(Default)]
pub struct RowsModelRust {
    kind: QString,
    rows: Vec<Row>,
}

impl qobject::RowsModel {
    fn model(&self) -> Option<Model> {
        model_of(&String::from(self.kind()))
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let (Some(model), Ok(row)) = (self.model(), usize::try_from(index.row())) else {
            return QVariant::default();
        };
        let name = usize::try_from(role - FIRST_ROLE)
            .ok()
            .and_then(|i| Row::role_names(model).get(i));
        let value = self
            .rust()
            .rows
            .get(row)
            .zip(name)
            .and_then(|(r, n)| r.value(n));
        match value {
            Some(RowValue::Str(s)) => QVariant::from(&QString::from(s)),
            Some(RowValue::Int(i)) => QVariant::from(&i),
            Some(RowValue::Bool(b)) => QVariant::from(&b),
            None => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        if let Some(model) = self.model() {
            for (i, name) in Row::role_names(model).iter().enumerate() {
                roles.insert(FIRST_ROLE + i as i32, QByteArray::from(*name));
            }
        }
        roles
    }

    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        // A list has no children.
        if parent.is_valid() {
            return 0;
        }
        self.rust().rows.len() as i32
    }

    /// Mirror one envio-ui operation, with the begin/end calls it needs.
    pub fn apply(mut self: Pin<&mut Self>, op: Op) {
        let len = self.rust().rows.len();
        let none = QModelIndex::default();
        match op {
            Op::Reset(rows) => unsafe {
                self.as_mut().begin_reset_model();
                self.as_mut().rust_mut().rows = rows;
                self.as_mut().end_reset_model();
            },
            Op::Insert { at, rows } if at <= len && !rows.is_empty() => unsafe {
                let last = at + rows.len() - 1;
                self.as_mut()
                    .begin_insert_rows(&none, at as i32, last as i32);
                self.as_mut().rust_mut().rows.splice(at..at, rows);
                self.as_mut().end_insert_rows();
            },
            Op::Update { at, row } if at < len => {
                self.as_mut().rust_mut().rows[at] = row;
                let index = self.index(at as i32, 0, &none);
                self.as_mut()
                    .data_changed(&index, &index, &QVector::<i32>::default());
            }
            Op::Remove { at, count } if count > 0 && at + count <= len => unsafe {
                self.as_mut()
                    .begin_remove_rows(&none, at as i32, (at + count - 1) as i32);
                self.as_mut().rust_mut().rows.drain(at..at + count);
                self.as_mut().end_remove_rows();
            },
            // An op that does not fit the rows is envio-ui's bug; say so
            // instead of corrupting the model.
            // (Never the rows: they hold subjects and addresses.)
            _ => log::warn!("ignored a model op that does not fit the {len} rows"),
        }
    }
}

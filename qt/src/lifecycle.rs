//! Starting and stopping the core, and handing models to it.

use std::pin::Pin;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QDateTime, QString, QTimeZone};
use envio_core::{Config, Core, Roots, RootsSource, paths};
use envio_ui::Op;

use crate::core_object::qobject;
use crate::models::qobject::RowsModel;
use crate::postal::PostalPoster;
use crate::sink::QtSink;

/// The wall clock and the local UTC offset, for `Ui` (which has no clock).
pub(crate) fn clock() -> (i64, i32) {
    let now = QDateTime::current_secs_since_epoch();
    let zone = QTimeZone::system_time_zone();
    let offset = zone.as_ref().map_or(0, |z| {
        z.offset_from_utc(&QDateTime::from_secs_since_epoch(now, z))
    });
    (now, offset)
}

fn log_roots(roots: &Roots) {
    let source = match roots.source() {
        RootsSource::System => "system",
        RootsSource::Bundled => "bundled",
        RootsSource::Empty => "empty",
    };
    log::warn!("CA roots: source {source}, {} certificates", roots.len());
    for error in roots.load_errors() {
        log::warn!("CA roots: {error}");
    }
}

impl qobject::EnvioCore {
    /// Start the core: folders, roots, worker. `Started` arrives later; if
    /// the core cannot start, `startFailed` says so.
    pub fn start(mut self: Pin<&mut Self>) {
        if self.rust().core.is_some() {
            return;
        }
        let env = |name: &str| std::env::var(name).ok();
        let (cache_dir, data_dir) = paths::app_dirs(&env);
        let roots = Roots::system_or_bundled();
        let config = Config {
            cache_dir,
            data_dir,
            roots: roots.clone(),
            oauth: Vec::new(),
            dns: None,
            log_level: log::LevelFilter::Info,
            foreground: true,
        };
        let sink = Arc::new(QtSink::new(self.qt_thread()));
        match Core::start(config, sink) {
            Ok(core) => {
                core.set_poster(Box::new(PostalPoster));
                // After the start: the file logger is up only now (#37).
                log_roots(&roots);
                self.as_mut().rust_mut().core = Some(core);
            }
            Err(error) => {
                eprintln!("envio: the core did not start: {error}");
                log::error!("the core did not start: {error}");
                self.start_failed(&QString::from(envio_core::start_failed()));
            }
        }
    }

    /// Flush unsent work and stop the worker (also done when QML drops
    /// the object).
    pub fn stop(mut self: Pin<&mut Self>) {
        if let Some(core) = self.as_mut().rust_mut().core.take() {
            core.shutdown();
        }
    }

    /// Hand over a model; it starts from the rows `Ui` holds now, so a
    /// model made late (or made again) is in step.
    ///
    /// # Safety
    ///
    /// `model` is null or points to a live `RowsModel`.
    pub unsafe fn attach_model(mut self: Pin<&mut Self>, model: *mut RowsModel) {
        // SAFETY: by the contract above; QML owns the model.
        let Some(model) = (unsafe { model.as_mut() }) else {
            return;
        };
        // SAFETY: the model stays where Qt put it.
        let mut model = unsafe { Pin::new_unchecked(model) };
        let kind = String::from(model.as_ref().kind());
        let Some(which) = crate::models::model_of(&kind) else {
            log::warn!("a RowsModel with unknown kind {kind:?} was not attached");
            return;
        };
        let thread = Box::new(model.as_mut().qt_thread());
        let rows = self.rust().ui.rows(which).to_vec();
        if thread.queue(move |m| m.apply(Op::Reset(rows))).is_err() {
            log::warn!("the {which:?} model is gone; it was not attached");
            return;
        }
        self.as_mut().rust_mut().models.insert(which, thread);
    }
}

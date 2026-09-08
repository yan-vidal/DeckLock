//! Settings-only preview. Never constructs a session lock, controller or PAM client.
use crate::{
    config::{Config, Theme},
    i18n::I18n,
    ui,
};
use gtk::prelude::*;
use std::rc::Rc;
#[derive(Default)]
pub struct LivePreview {
    view: Option<ui::View>,
    signature: String,
    idle: bool,
}
impl LivePreview {
    pub fn set_idle(&mut self, idle: bool) {
        self.idle = idle;
    }
    pub fn is_open(&self) -> bool {
        self.view.as_ref().is_some_and(|v| v.window.is_visible())
    }
    pub fn close(&mut self) {
        if let Some(view) = self.view.take() {
            view.window.destroy();
        }
    }
    pub fn update(
        &mut self,
        app: &gtk::Application,
        mut config: Config,
        present: bool,
    ) -> Result<(), String> {
        if !present && !self.is_open() {
            return Ok(());
        }
        config.controller_socket = None;
        let mut theme = Theme::from_config(&config)?;
        if let Some(layout) = &config.layout {
            theme.layout = layout.clone();
        }
        // CSS providers are already replaced by settings. Palette-only edits need
        // no widget rebuild, preserving the playing video without a black flash.
        let mut structural = config.clone();
        structural.theme_preset.clear();
        structural.theme = None;
        let signature = format!(
            "{}{:?}{:?}{}",
            toml::to_string(&structural).map_err(|e| e.to_string())?,
            theme.layout,
            theme.background,
            self.idle
        );
        if self.signature == signature && self.is_open() {
            if present {
                self.view.as_ref().unwrap().window.present();
            }
            return Ok(());
        }
        let settings = Rc::new(ui::Settings {
            strings: I18n::new(config.locale.as_deref(), None)?,
            config,
            theme,
            preview: true,
            show_keyboard: false,
            start_idle: self.idle,
            username: crate::auth::current_username()?,
        });
        let view = if self.is_open() {
            ui::rebuild_preview(self.view.take().unwrap(), app, settings)
        } else {
            self.close();
            ui::build(app, settings, Rc::new(|_| {}))
        };
        view.set_preview_idle(self.idle);
        view.window.set_widget_name("settings-live-preview");
        if present {
            view.window.present();
        }
        self.view = Some(view);
        self.signature = signature;
        Ok(())
    }
}
impl Drop for LivePreview {
    fn drop(&mut self) {
        self.close();
    }
}

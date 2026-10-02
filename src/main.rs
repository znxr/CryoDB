#![windows_subsystem = "windows"]
mod ai;
mod app;
mod cli;
mod constants;
mod db;
mod i18n;
mod model;
mod storage;
mod tui;
mod ui;
mod update;
mod utils;

pub(crate) use crate::app::core::App;
pub(crate) use crate::app::features::workspace::diagram::model::*;
pub(crate) use crate::app::message::Message;
pub(crate) use crate::app::types::*;
pub(crate) use crate::constants::*;
pub(crate) use crate::db::DatabasePool;
pub(crate) use crate::model::connection::{
    ConnectionInfo, ConnectionStore, DatabaseDriver, StoredConnection, TlsMode,
};
pub(crate) use crate::model::diagram::*;
pub(crate) use crate::model::settings::{
    FontChoice, ResultGridDensity, Settings, SettingsStore, ShortcutBinding, SystemThemeMode,
    ThemeChoice, UiDensity,
};
pub(crate) use crate::model::table::*;

use crate::ai::AiProvider;

use crate::utils::fuzzy::normalize_omni_lookup;
use crate::utils::helpers::*;
use iced::{Color, Font};

fn app_title<T>(_state: &T) -> String {
    String::from("CryoDB")
}

fn app_window_settings() -> iced::window::Settings {
    let icon = image::load_from_memory_with_format(
        include_bytes!("../icons/icon.png"),
        image::ImageFormat::Png,
    )
    .expect("embedded application icon must be a valid PNG")
    .to_rgba8();
    let platform_specific = {
        #[cfg(target_os = "linux")]
        {
            iced::window::settings::PlatformSpecific {
                application_id: String::from("cryodb"),
                ..Default::default()
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Default::default()
        }
    };

    iced::window::Settings {
        icon: Some(
            iced::window::icon::from_rgba(icon.to_vec(), icon.width(), icon.height())
                .expect("embedded application icon must have valid dimensions"),
        ),
        platform_specific,
        ..Default::default()
    }
}

pub fn main() -> iced::Result {
    if cli::should_run_cli() {
        std::process::exit(cli::run());
    }

    iced::application(App::new, App::update, App::view)
        .title(app_title::<App>)
        .window(app_window_settings())
        .subscription(App::subscription)
        .theme(App::theme)
        .antialiasing(true)
        .default_font(Font::MONOSPACE)
        .font(include_bytes!("../fonts/remixicon.ttf").as_slice())
        .font(include_bytes!("../fonts/devicon.ttf").as_slice())
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::features::onboarding::LAST_STEP as ONBOARDING_LAST_STEP;

    #[test]
    fn settings_default_smoke() {
        let settings = Settings::default();
        assert!(!settings.omni_prefix.is_empty());
    }

    #[test]
    fn app_window_has_icon() {
        let settings = app_window_settings();
        assert!(settings.icon.is_some());

        #[cfg(target_os = "linux")]
        assert_eq!(settings.platform_specific.application_id, "cryodb");
    }

    #[test]
    fn app_new_smoke() {
        let _ = App::new();
    }

    #[test]
    fn login_view_smoke() {
        let (app, _) = App::new();
        let _ = app.login_view();
    }

    #[test]
    fn view_smoke() {
        let (app, _) = App::new();
        let _ = app.view();
    }

    #[test]
    fn schema_diagram_view_smoke() {
        let (mut app, _) = App::new();
        app.onboarding.step = None;
        let _ = app.open_schema_diagram();
        let state = app
            .workspace
            .diagram
            .active_diagram_mut()
            .expect("diagram state");
        state.loading = false;
        state.diagram = SchemaDiagram::build(
            vec![(
                String::from("users"),
                vec![DiagramColumn {
                    name: String::from("id"),
                    data_type: String::from("INTEGER"),
                    primary: true,
                    foreign: false,
                    nullable: false,
                    default: None,
                    indexed: true,
                    unique: true,
                    attributes: String::new(),
                }],
            )],
            &[],
        );
        state.record_change(SchemaChange::AddColumn {
            table: String::from("users"),
            column: String::from("name"),
            data_type: String::from("TEXT"),
        });
        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Diagram(
                crate::app::features::workspace::diagram::Message::DeleteSavedDiagram,
            ),
        ));
        assert_eq!(
            app.active_diagram()
                .and_then(|state| state.modal.as_ref())
                .map(|modal| modal.kind),
            Some(DiagramModalKind::DeleteSavedDiagram)
        );
        let _ = app.view();
    }

    #[test]
    fn onboarding_view_smoke() {
        let (mut app, _) = App::new();
        for step in 0..=ONBOARDING_LAST_STEP {
            app.onboarding.step = Some(step);
            let _ = app.view();
        }
    }

    #[test]
    fn leaving_onboarding_changes_the_root_widget() {
        use iced::advanced::widget::Tree;

        let (mut app, _) = App::new();
        app.onboarding.step = Some(ONBOARDING_LAST_STEP);
        let onboarding = Tree::new(app.view().as_widget());

        app.onboarding.step = None;
        let regular = Tree::new(app.view().as_widget());

        assert_ne!(onboarding.tag, regular.tag);
    }
}

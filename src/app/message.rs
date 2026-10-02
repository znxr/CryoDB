#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub(crate) enum Message {
    Shell(crate::app::shell::Message),
    Connections(crate::app::features::connections::Message),
    Workspace(crate::app::features::workspace::Message),
    Ai(crate::app::features::ai::Message),
    Transfer(crate::app::features::transfer::Message),
    Settings(crate::app::features::settings::Message),
    Updater(crate::app::features::updater::Message),
    Onboarding(crate::app::features::onboarding::Message),
}

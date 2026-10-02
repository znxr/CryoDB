use crate::app::types::ToastLevel;
use iced::Task;
use std::sync::Arc;

mod view;

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Check { announce: bool },
    CheckFinished(Result<crate::update::Status, String>),
    Install,
    DownloadFinished(Result<Arc<Vec<u8>>, String>),
    DismissBanner,
}

#[derive(Debug)]
pub(crate) enum Output {
    Toast(ToastLevel, String),
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) status: Status,
    banner_dismissed: bool,
    check_announces: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum Status {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(crate::update::Available),
    Downloading(String),
    Failed,
}

impl Status {
    pub(crate) fn label(&self) -> String {
        match self {
            Status::Idle => String::from("No check run yet."),
            Status::Checking => String::from("Checking for updates..."),
            Status::UpToDate => String::from("You are on the latest release."),
            Status::Available(available) => crate::i18n::tr_with(
                "CryoDB {version} is available.",
                &[("{version}", &available.version.to_string())],
            ),
            Status::Downloading(version) => crate::i18n::tr_with(
                "Downloading CryoDB {version}...",
                &[("{version}", &version.to_string())],
            ),
            Status::Failed => String::from("The last update check failed."),
        }
    }

    pub(crate) fn available(&self) -> Option<&crate::update::Available> {
        match self {
            Status::Available(available) => Some(available),
            _ => None,
        }
    }

    pub(crate) fn is_busy(&self) -> bool {
        matches!(self, Status::Checking | Status::Downloading(_))
    }
}

impl State {
    pub(crate) fn update(&mut self, message: Message) -> (Task<Message>, Option<Output>) {
        let mut output = None;
        let task = match message {
            Message::Check { announce } => {
                if self.status.is_busy() {
                    return (Task::none(), output);
                }
                self.status = Status::Checking;
                self.banner_dismissed = false;
                self.check_announces = announce;
                Task::perform(
                    async {
                        let http = update_http_client()?;
                        crate::update::check(&http).await
                    },
                    Message::CheckFinished,
                )
            }
            Message::CheckFinished(result) => {
                let announce = std::mem::take(&mut self.check_announces);
                self.status = match result {
                    Ok(crate::update::Status::Available(available)) => Status::Available(available),
                    Ok(crate::update::Status::UpToDate) => {
                        if announce {
                            output = Some(Output::Toast(
                                ToastLevel::Success,
                                crate::i18n::tr_with(
                                    "CryoDB {version} is the latest release.",
                                    &[("{version}", crate::update::current_version())],
                                ),
                            ));
                        }
                        Status::UpToDate
                    }
                    Ok(crate::update::Status::Disabled) => {
                        if announce {
                            output = Some(Output::Toast(
                                ToastLevel::Info,
                                "Auto-update is not configured for this build.".into(),
                            ));
                        }
                        Status::Idle
                    }
                    Err(error) => {
                        if announce {
                            output = Some(Output::Toast(ToastLevel::Error, error.clone()));
                        }
                        Status::Failed
                    }
                };
                Task::none()
            }
            Message::Install => {
                let Some(available) = self.status.available().cloned() else {
                    return (Task::none(), output);
                };
                if !available.self_updatable() {
                    output = Some(Output::Toast(
                        ToastLevel::Info,
                        available.install.manual_hint().into(),
                    ));
                    return (Task::none(), output);
                }
                self.status = Status::Downloading(available.version.clone());
                Task::perform(
                    async move {
                        let http = update_http_client()?;
                        crate::update::download_verified(&http, &available.artifact)
                            .await
                            .map(Arc::new)
                    },
                    Message::DownloadFinished,
                )
            }
            Message::DownloadFinished(result) => {
                let bytes = match result {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        output = Some(Output::Toast(ToastLevel::Error, error.clone()));
                        self.status = Status::Failed;
                        return (Task::none(), output);
                    }
                };
                let install = crate::update::detect_install();
                let bytes = Arc::try_unwrap(bytes).unwrap_or_else(|shared| shared.to_vec());
                if let Err(error) = crate::update::apply_and_restart(&install, bytes) {
                    output = Some(Output::Toast(ToastLevel::Error, error.clone()));
                    self.status = Status::Failed;
                }
                Task::none()
            }
            Message::DismissBanner => {
                self.banner_dismissed = true;
                Task::none()
            }
        };
        (task, output)
    }
}

fn update_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(10))
        .user_agent(concat!("CryoDB/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("update client setup failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{Message, Output, State, Status};
    use crate::app::types::ToastLevel;
    use crate::update::{Artifact, Available, Install, Status as CheckStatus};

    fn available(install: Install) -> Available {
        Available {
            version: String::from("99.0.0"),
            artifact: Artifact {
                url: String::from("https://example.invalid/update"),
                sha256: String::new(),
                size: 0,
            },
            install,
        }
    }

    #[test]
    fn startup_checks_are_silent_but_manual_checks_announce() {
        for announce in [false, true] {
            for result in [
                Ok(CheckStatus::UpToDate),
                Ok(CheckStatus::Disabled),
                Err(String::from("offline")),
            ] {
                let mut state = State::default();
                let _ = state.update(Message::Check { announce });
                let (_, output) = state.update(Message::CheckFinished(result.clone()));
                assert_eq!(output.is_some(), announce);
                assert!(!state.check_announces);
                match result {
                    Ok(CheckStatus::UpToDate) => {
                        assert!(matches!(state.status, Status::UpToDate));
                        if announce {
                            assert!(matches!(
                                output,
                                Some(Output::Toast(ToastLevel::Success, _))
                            ));
                        }
                    }
                    Ok(CheckStatus::Disabled) => assert!(matches!(state.status, Status::Idle)),
                    Err(_) => assert!(matches!(state.status, Status::Failed)),
                    _ => unreachable!(),
                }
            }
        }
    }

    #[test]
    fn checks_do_not_interrupt_busy_operations() {
        for status in [
            Status::Checking,
            Status::Downloading(String::from("99.0.0")),
        ] {
            let mut state = State {
                status,
                banner_dismissed: true,
                check_announces: false,
            };
            let (_, output) = state.update(Message::Check { announce: true });
            assert!(state.status.is_busy());
            assert!(state.banner_dismissed);
            assert!(!state.check_announces);
            assert!(output.is_none());
        }
    }

    #[test]
    fn a_new_check_restores_the_dismissed_banner() {
        let mut state = State::default();
        let _ = state.update(Message::CheckFinished(Ok(CheckStatus::Available(
            available(Install::Other),
        ))));
        assert!(matches!(state.status, Status::Available(_)));
        let _ = state.update(Message::DismissBanner);
        assert!(state.banner_dismissed);
        let _ = state.update(Message::Check { announce: true });
        assert!(matches!(state.status, Status::Checking));
        assert!(!state.banner_dismissed);
    }

    #[test]
    fn managed_installs_remain_available_and_report_manual_instructions() {
        let mut state = State {
            status: Status::Available(available(Install::Pacman)),
            ..State::default()
        };
        let (_, output) = state.update(Message::Install);
        assert!(matches!(state.status, Status::Available(_)));
        assert!(
            matches!(output, Some(Output::Toast(ToastLevel::Info, text)) if text == Install::Pacman.manual_hint())
        );
    }

    #[test]
    fn failed_downloads_report_errors_and_allow_retrying_the_check() {
        let mut state = State {
            status: Status::Available(available(Install::AppImage(
                "/tmp/cryodb-test.AppImage".into(),
            ))),
            ..State::default()
        };
        let (_, output) = state.update(Message::Install);
        assert!(matches!(&state.status, Status::Downloading(version) if version == "99.0.0"));
        assert!(output.is_none());
        let (_, output) = state.update(Message::DownloadFinished(Err(String::from(
            "checksum mismatch",
        ))));
        assert!(matches!(state.status, Status::Failed));
        assert!(
            matches!(output, Some(Output::Toast(ToastLevel::Error, text)) if text == "checksum mismatch")
        );
        let _ = state.update(Message::Check { announce: true });
        assert!(matches!(state.status, Status::Checking));
    }
}

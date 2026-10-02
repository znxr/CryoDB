use super::super::forms::history_input;
use super::super::{Message, State};
use crate::constants::{ICON_AI, ICON_CHECK_FILL, ICON_CLOSE_LINE, MODAL_SCROLLBAR_GUTTER};
use crate::model::table::{
    PostgresRoleModalMode, PostgresRoleModalTab, PostgresRoleTextField, PostgresRoleToggleField,
};
use crate::ui::ids::text_field_id;
use crate::ui::presentation::{LayoutMode, Presentation};
use crate::ui::styles::{
    compact_button_style, compact_input_style, compact_primary_button_style,
    compact_secondary_button_style, modal_backdrop_style, panel_border_style, panel_style,
    settings_modal_tab_button_style,
};
use crate::ui::theme::ThemeChoice;
use iced::widget::{
    button, checkbox, container, mouse_area, row, scrollable, space, stack, text, text_editor,
};
use iced::{Center, Element, Fill, Length, mouse};

pub(crate) struct View<'a> {
    pub(crate) state: &'a State,
    pub(crate) presentation: Presentation<'a>,
    pub(crate) layout_mode: LayoutMode,
    pub(crate) theme_choice: ThemeChoice,
    pub(crate) spinner: char,
    pub(crate) is_generating_ai: bool,
}

impl<'a> View<'a> {
    pub(crate) fn postgres_role_modal_view(&self) -> Element<'a, Message> {
        let Some(modal_state) = self.state.postgres_role_modal.as_ref() else {
            return space::horizontal().into();
        };

        let layout = self.layout_mode;
        let modal_width = self.presentation.scale_f32(760.0);
        let modal_height = self.presentation.scale_f32(560.0);
        let modal_padding = self.presentation.scale_u16(12);
        let draft = &modal_state.draft;

        let title = match modal_state.mode {
            PostgresRoleModalMode::Create => String::from("Create Login / Group Role"),
            PostgresRoleModalMode::Properties => {
                if let Some(source) = modal_state.source_role_name.as_ref() {
                    format!("{}: {source}", crate::i18n::tr("Role Properties"))
                } else {
                    String::from("Role Properties")
                }
            }
        };

        let tab_button = |tab: PostgresRoleModalTab| {
            let is_selected = modal_state.active_tab == tab;
            button(self.presentation.button_text(tab.label()))
                .padding(self.presentation.button_padding_tight())
                .style(move |theme, status| {
                    settings_modal_tab_button_style(is_selected, theme, status)
                })
                .on_press(Message::PostgresRoleModalTabSelected(tab))
        };

        let mut tab_row = row![]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center);
        for tab in PostgresRoleModalTab::ordered() {
            tab_row = tab_row.push(tab_button(tab));
        }

        let body: Element<'_, Message> = match modal_state.active_tab {
            PostgresRoleModalTab::General => iced::widget::column![
                self.presentation.label_text("Role name"),
                history_input(text_field_id("pg-role-name"), "Role name", &draft.role_name, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::RoleName,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
                checkbox(draft.can_login)
                    .label(crate::i18n::tr("Can login"))
                    .on_toggle(|enabled| Message::PostgresRoleModalToggleChanged {
                        field: PostgresRoleToggleField::CanLogin,
                        enabled,
                    })
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
                self.presentation.label_text("Connection limit (-1 = unlimited)"),
                history_input(text_field_id("pg-role-connection-limit"), "-1", &draft.connection_limit, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::ConnectionLimit,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
                self.presentation.label_text("Comment"),
                history_input(text_field_id("pg-role-comment"), "Role comment", &draft.comment, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::Comment,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
            ]
            .spacing(self.presentation.scale_u16(8))
            .width(Fill)
            .into(),
            PostgresRoleModalTab::Definition => iced::widget::column![
                self.presentation.label_text("Password"),
                history_input(text_field_id("pg-role-password"), "Leave empty to keep unchanged", &draft.password, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::Password,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .secure(true)
                    .width(Fill),
                self.presentation.label_text("Valid until (timestamp or empty for infinity)"),
                history_input(text_field_id("pg-role-valid-until"), "YYYY-MM-DD HH:MM:SS+00", &draft.valid_until, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::ValidUntil,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
            ]
            .spacing(self.presentation.scale_u16(8))
            .width(Fill)
            .into(),
            PostgresRoleModalTab::Privileges => iced::widget::column![
                checkbox(draft.is_superuser)
                    .label(crate::i18n::tr("Superuser"))
                    .on_toggle(|enabled| Message::PostgresRoleModalToggleChanged {
                        field: PostgresRoleToggleField::IsSuperuser,
                        enabled,
                    })
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
                checkbox(draft.can_create_db)
                    .label(crate::i18n::tr("Can create databases"))
                    .on_toggle(|enabled| Message::PostgresRoleModalToggleChanged {
                        field: PostgresRoleToggleField::CanCreateDb,
                        enabled,
                    })
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
                checkbox(draft.can_create_role)
                    .label(crate::i18n::tr("Can create roles"))
                    .on_toggle(|enabled| Message::PostgresRoleModalToggleChanged {
                        field: PostgresRoleToggleField::CanCreateRole,
                        enabled,
                    })
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
                checkbox(draft.can_replicate)
                    .label(crate::i18n::tr("Can replicate"))
                    .on_toggle(|enabled| Message::PostgresRoleModalToggleChanged {
                        field: PostgresRoleToggleField::CanReplicate,
                        enabled,
                    })
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
                checkbox(draft.bypass_rls)
                    .label(crate::i18n::tr("Bypass row-level security"))
                    .on_toggle(|enabled| Message::PostgresRoleModalToggleChanged {
                        field: PostgresRoleToggleField::BypassRls,
                        enabled,
                    })
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
            ]
            .spacing(self.presentation.scale_u16(8))
            .width(Fill)
            .into(),
            PostgresRoleModalTab::Membership => iced::widget::column![
                self.presentation.label_text("Member of roles (comma-separated)"),
                history_input(text_field_id("pg-role-member-of"), "readonly, analytics", &draft.member_of_roles, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::MemberOfRoles,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
                self.presentation.label_text("Members of this role (comma-separated)"),
                history_input(text_field_id("pg-role-members"), "app_user, etl_user", &draft.members, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::Members,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
            ]
            .spacing(self.presentation.scale_u16(8))
            .width(Fill)
            .into(),
            PostgresRoleModalTab::Parameters => iced::widget::column![
                self.presentation.label_text("search_path"),
                history_input(text_field_id("pg-role-search-path"), "public", &draft.search_path, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::SearchPath,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
                self.presentation.label_text("work_mem"),
                history_input(text_field_id("pg-role-work-mem"), "4MB", &draft.work_mem, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::WorkMem,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
                self.presentation.label_text("maintenance_work_mem"),
                history_input(text_field_id("pg-role-maintenance-work-mem"), "64MB", &draft.maintenance_work_mem, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::MaintenanceWorkMem,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
                self.presentation.label_text("statement_timeout"),
                history_input(text_field_id("pg-role-statement-timeout"), "30s", &draft.statement_timeout, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::StatementTimeout,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
                self.presentation.label_text("lock_timeout"),
                history_input(text_field_id("pg-role-lock-timeout"), "5s", &draft.lock_timeout, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::LockTimeout,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
                self.presentation.label_text("idle_in_transaction_session_timeout"),
                history_input(text_field_id("pg-role-idle-timeout"), "60s", &draft.idle_in_transaction_session_timeout, |value| Message::PostgresRoleModalTextChanged {
                        field: PostgresRoleTextField::IdleInTransactionSessionTimeout,
                        value,
                    })
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill),
            ]
            .spacing(self.presentation.scale_u16(8))
            .width(Fill)
            .into(),
            PostgresRoleModalTab::Security => iced::widget::column![
                checkbox(draft.inherit_privileges)
                    .label(crate::i18n::tr("Inherit privileges"))
                    .on_toggle(|enabled| Message::PostgresRoleModalToggleChanged {
                        field: PostgresRoleToggleField::InheritPrivileges,
                        enabled,
                    })
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
                self.presentation.label_text(
                    "Controls whether this role automatically inherits privileges from granted roles.",
                )
                .wrapping(text::Wrapping::Word),
            ]
            .spacing(self.presentation.scale_u16(8))
            .width(Fill)
            .into(),
            PostgresRoleModalTab::Sql => {
                let ai_icon = if self.is_generating_ai {
                    self.spinner
                } else {
                    ICON_AI
                };
                let ai_button = button(self.presentation.icon_text(ai_icon))
                    .padding(self.presentation.button_padding_icon())
                    .style(compact_button_style);
                let ai_button: Element<'_, Message> = if self.is_generating_ai {
                    ai_button.into()
                } else {
                    ai_button
                        .on_press(Message::PostgresRoleModalGenerateSqlWithAi)
                        .into()
                };
                let ai_button: Element<'_, Message> =
                    self.with_tooltip(ai_button, "Generate SQL with AI");

                let sql_editor = text_editor(&modal_state.sql_editor)
                    .on_action(Message::PostgresRoleModalSqlAction)
                    .wrapping(text::Wrapping::WordOrGlyph)
                    .highlight("sql", self.theme_choice.highlighter())
                    .font(crate::ui::presentation::font_for_choice(
                        &self.presentation.settings.editor_font,
                    ))
                    .size(self.presentation.input_text_size())
                    .padding(self.presentation.scale_f32(5.0))
                    .height(Fill);

                iced::widget::column![
                    row![self.presentation.label_text("Editable SQL"), space::horizontal(), ai_button]
                        .spacing(self.presentation.scale_u16(6))
                        .align_y(Center),
                    container(sql_editor)
                        .width(Fill)
                        .height(Fill)
                        .style(panel_border_style),
                ]
                .spacing(self.presentation.scale_u16(8))
                .width(Fill)
                .height(Fill)
                .into()
            }
        };

        let body = if modal_state.active_tab == PostgresRoleModalTab::Sql {
            body
        } else {
            scrollable(container(body).width(Fill))
                .height(Fill)
                .width(Fill)
                .direction(iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::new()
                        .spacing(self.presentation.scale_f32(MODAL_SCROLLBAR_GUTTER)),
                ))
                .into()
        };

        let close_button = button(
            self.presentation
                .action_label("Close", ICON_CLOSE_LINE, layout),
        )
        .padding(self.presentation.button_padding())
        .style(compact_button_style)
        .on_press(Message::ClosePostgresRoleModal);

        let reset_button: Element<'_, Message> = {
            let button = button(self.presentation.button_text("Reset"))
                .padding(self.presentation.button_padding())
                .style(compact_secondary_button_style);
            if self.state.postgres_role_action_running {
                button.into()
            } else {
                button.on_press(Message::PostgresRoleModalReset).into()
            }
        };

        let save_label = if self.state.postgres_role_action_running {
            "Saving..."
        } else if modal_state.mode == PostgresRoleModalMode::Create {
            "Create role"
        } else {
            "Save role"
        };
        let save_icon = if self.state.postgres_role_action_running {
            self.spinner
        } else {
            ICON_CHECK_FILL
        };
        let save_button: Element<'_, Message> = {
            let button = button(
                self.presentation
                    .action_label(save_label, save_icon, layout),
            )
            .padding(self.presentation.button_padding())
            .style(compact_primary_button_style);
            if self.state.postgres_role_action_running {
                button.into()
            } else {
                button.on_press(Message::PostgresRoleModalSave).into()
            }
        };

        let header = row![
            self.presentation.heading_text(title),
            space::horizontal(),
            close_button
        ]
        .spacing(self.presentation.scale_u16(8))
        .align_y(Center);

        let footer = row![reset_button, space::horizontal(), save_button]
            .spacing(self.presentation.scale_u16(8))
            .align_y(Center);

        let content = iced::widget::column![header, tab_row, body, footer]
            .spacing(self.presentation.scale_u16(10))
            .width(Fill)
            .height(Fill);

        let modal = container(content)
            .padding(modal_padding)
            .width(Length::Fixed(modal_width))
            .height(Length::Fixed(modal_height))
            .style(panel_style);

        let dim = self.presentation.settings.modal_backdrop_dim;
        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(move |theme| modal_backdrop_style(theme, dim)),
        )
        .on_press(Message::ModalBlocked)
        .on_scroll(|_| Message::ModalBlocked)
        .interaction(mouse::Interaction::Idle);

        let modal_layer = container(modal).width(Fill).height(Fill).center(Fill);

        stack![backdrop, modal_layer]
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn with_tooltip(
        &self,
        element: impl Into<Element<'a, Message>>,
        text: &str,
    ) -> Element<'a, Message> {
        mouse_area(element.into())
            .on_enter(Message::Tooltip(Some(crate::i18n::tr(text))))
            .on_exit(Message::Tooltip(None))
            .interaction(mouse::Interaction::Pointer)
            .into()
    }
}

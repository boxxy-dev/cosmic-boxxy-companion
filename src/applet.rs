use crate::config::{CompanionConfig, SLOT_COUNT};
use crate::dbus::{
    DaemonEvent, cancel_pending_task, daemon_subscription, fetch_detached_sessions_standalone,
    fetch_pending_tasks_standalone, request_focus_pane, request_restore_detached,
    terminate_detached_session,
};
use crate::launcher::{
    get_char_avatar_path, load_circular_avatar_pixels, present_or_launch_terminal,
    spawn_agent_command,
};
use crate::models::{CharacterInfo, DetachedSessionInfo, PendingTaskInfo, parse_hex_color};

use cosmic::app::{Core, Task};
use cosmic::applet::token::subscription::{
    TokenRequest, TokenUpdate, activation_token_subscription,
};
use cosmic::applet::{menu_button, padded_control};
use cosmic::iced::alignment::{Alignment, Horizontal, Vertical};
use cosmic::iced::window::Id;
use cosmic::iced::{Background, Border, Color, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{self, button, column, container, divider, icon, row, scrollable, text, text_input};
use cosmic::{Element, theme};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const APP_ID: &str = "dev.boxxy.CosmicBoxxyCompanion";

const ICON_ON_BYTES: &[u8] = include_bytes!("../resources/icons/boxxyclaw-symbolic.svg");
const ICON_OFF_BYTES: &[u8] = include_bytes!("../resources/icons/boxxyclaw-off-symbolic.svg");

const AVATAR_DISPLAY_SIZE: f32 = 24.0;
const AVATAR_DECODE_PX: u32 = 48;
const COLOR_DOT_SIZE: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopupPage {
    #[default]
    Main,
    Shortcuts,
}

#[derive(Debug, Clone)]
pub enum Message {
    PopupClosed(Id),
    TogglePopup(cosmic::surface::Action<Message>, bool),
    Surface(cosmic::surface::Action<Message>),
    DaemonEvent(DaemonEvent),
    TokenUpdate(TokenUpdate),
    FocusCharacter(String),
    RestoreDetached(u32),
    KillDetached(u32),
    CancelTask(String, String),
    CountdownTick,
    ToggleAgent,
    RestartAgent,
    OpenShortcutsPage,
    CloseShortcutsPage,
    CycleSlotCharacter(usize),
    SlotShortcutChanged(usize, String),
    ClearSlot(usize),
    Noop,
}

pub struct BoxxyApplet {
    core: Core,
    popup: Option<Id>,
    popup_page: PopupPage,
    daemon_running: bool,
    catalog: Vec<CharacterInfo>,
    detached_sessions: Vec<DetachedSessionInfo>,
    pending_tasks: Vec<PendingTaskInfo>,
    now_unix: i64,
    avatar_cache: HashMap<String, icon::Handle>,
    config: CompanionConfig,
    home_dir: Option<PathBuf>,
    calloop_token_tx: Option<cosmic::cctk::sctk::reexports::calloop::channel::Sender<TokenRequest>>,
}

fn current_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Default for BoxxyApplet {
    fn default() -> Self {
        Self {
            core: Core::default(),
            popup: None,
            popup_page: PopupPage::Main,
            daemon_running: false,
            catalog: Vec::new(),
            detached_sessions: Vec::new(),
            pending_tasks: Vec::new(),
            now_unix: current_unix_secs(),
            avatar_cache: HashMap::new(),
            config: CompanionConfig::load(),
            home_dir: dirs::home_dir(),
            calloop_token_tx: None,
        }
    }
}

impl BoxxyApplet {
    fn close_popup_task(&mut self) -> Task<Message> {
        self.popup_page = PopupPage::Main;
        if let Some(id) = self.popup.take() {
            cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)))
        } else {
            Task::none()
        }
    }

    fn request_terminal_activation(&self) {
        if let Some(tx) = &self.calloop_token_tx {
            let _ = tx.send(TokenRequest {
                app_id: "dev.boxxy.BoxxyTerminal".to_string(),
                exec: "present-terminal".to_string(),
            });
        } else {
            present_or_launch_terminal(None);
        }
    }

    fn refresh_avatars_async(catalog: Vec<CharacterInfo>) -> HashMap<String, icon::Handle> {
        let mut map = HashMap::new();
        for info in catalog {
            if info.has_avatar
                && let Some(path) = get_char_avatar_path(&info.config.name)
                && let Some((w, h, pixels)) = load_circular_avatar_pixels(&path, AVATAR_DECODE_PX)
            {
                map.insert(
                    info.config.name.clone(),
                    icon::from_raster_pixels(w, h, pixels),
                );
            }
        }
        map
    }

    fn view_popup(&self) -> Element<'_, Message> {
        match self.popup_page {
            PopupPage::Main => self.view_main_popup(),
            PopupPage::Shortcuts => self.view_shortcuts_popup(),
        }
    }

    fn view_main_popup(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let mut content = column::with_capacity(12)
            .padding([8, 0])
            .width(Length::Fill);

        // 1. Daemon Status Row
        let (status_text, status_color) = if self.daemon_running {
            (
                "Boxxy Agent — Running",
                Color::from_rgb8(0x33, 0xd1, 0x7a),
            )
        } else {
            (
                "Boxxy Agent — Stopped",
                Color::from_rgb8(0xe5, 0xa5, 0x0a),
            )
        };

        let status_dot = container(widget::space::horizontal().width(Length::Fixed(8.0)))
            .width(Length::Fixed(8.0))
            .height(Length::Fixed(8.0))
            .class(theme::Container::custom(move |_| container::Style {
                background: Some(Background::Color(status_color)),
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }));

        let status_row = padded_control(
            row::with_capacity(2)
                .align_y(Alignment::Center)
                .spacing(spacing.space_xs)
                .push(status_dot)
                .push(
                    text::body(status_text).class(theme::Text::Color(status_color)),
                ),
        );
        content = content.push(status_row);

        // 2. Detached Background Shells section
        if self.daemon_running && !self.detached_sessions.is_empty() {
            content = content.push(
                padded_control(divider::horizontal::default())
                    .padding([spacing.space_xxs, spacing.space_s]),
            );

            let count = self.detached_sessions.len();
            let header_label = text::caption(format!("Background Shells ({count})"))
                .class(theme::Text::Color(Color::from_rgb8(0x9a, 0x99, 0x96)));

            let restore_all_btn = Self::action_pill_button(
                "Restore All",
                Message::RestoreDetached(0),
                PillKind::Restore,
            );
            let kill_all_btn =
                Self::action_pill_button("Kill All", Message::KillDetached(0), PillKind::Kill);

            let header_row = padded_control(
                row::with_capacity(4)
                    .align_y(Alignment::Center)
                    .spacing(spacing.space_xxs)
                    .push(header_label)
                    .push(widget::space::horizontal().width(Length::Fill))
                    .push(restore_all_btn)
                    .push(kill_all_btn),
            );
            content = content.push(header_row);

            let mut shells_col = column::with_capacity(self.detached_sessions.len())
                .spacing(spacing.space_xxxs)
                .width(Length::Fill);

            for session in &self.detached_sessions {
                shells_col = shells_col.push(self.view_detached_row(session));
            }

            content = content.push(shells_col);
        }

        // 2b. Pending Reminders section
        if self.daemon_running && !self.pending_tasks.is_empty() {
            content = content.push(
                padded_control(divider::horizontal::default())
                    .padding([spacing.space_xxs, spacing.space_s]),
            );

            let count = self.pending_tasks.len();
            let header_row = padded_control(
                text::caption(format!("Pending Reminders ({count})"))
                    .class(theme::Text::Color(Color::from_rgb8(0x9a, 0x99, 0x96))),
            );
            content = content.push(header_row);

            let mut tasks_col = column::with_capacity(self.pending_tasks.len())
                .spacing(spacing.space_xxxs)
                .width(Length::Fill);

            for task in &self.pending_tasks {
                tasks_col = tasks_col.push(self.view_pending_task_row(task));
            }

            content = content.push(tasks_col);
        }

        // 3. Character Roster section
        if self.daemon_running && !self.catalog.is_empty() {
            content = content.push(
                padded_control(divider::horizontal::default())
                    .padding([spacing.space_xxs, spacing.space_s]),
            );

            let char_header = padded_control(
                text::caption(format!("Characters ({})", self.catalog.len()))
                    .class(theme::Text::Color(Color::from_rgb8(0x9a, 0x99, 0x96))),
            );
            content = content.push(char_header);

            let mut chars_col = column::with_capacity(self.catalog.len())
                .spacing(2)
                .width(Length::Fill);

            for info in &self.catalog {
                chars_col = chars_col.push(self.view_character_row(info));
            }

            let chars_element: Element<'_, Message> = if self.catalog.len() > 8 {
                scrollable(chars_col)
                    .height(Length::Fixed(280.0))
                    .into()
            } else {
                chars_col.into()
            };
            content = content.push(chars_element);
        }

        // 4. Daemon Control Actions
        content = content.push(
            padded_control(divider::horizontal::default())
                .padding([spacing.space_xxs, spacing.space_s]),
        );

        let toggle_label = if self.daemon_running {
            "Stop Agent"
        } else {
            "Start Agent"
        };
        content = content.push(
            menu_button(text::body(toggle_label)).on_press(Message::ToggleAgent),
        );

        if self.daemon_running {
            content = content.push(
                menu_button(text::body("Restart Agent")).on_press(Message::RestartAgent),
            );
        }

        // 5. Shortcut Settings
        content = content.push(
            padded_control(divider::horizontal::default())
                .padding([spacing.space_xxs, spacing.space_s]),
        );

        content = content.push(
            menu_button(
                row::with_capacity(3)
                    .align_y(Alignment::Center)
                    .push(text::body("Shortcut Settings"))
                    .push(widget::space::horizontal().width(Length::Fill))
                    .push(
                        icon::from_name("go-next-symbolic")
                            .symbolic(true)
                            .size(16),
                    ),
            )
            .on_press(Message::OpenShortcutsPage),
        );

        content.into()
    }

    fn view_detached_row<'a>(&'a self, session: &'a DetachedSessionInfo) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let title_str = session.title_text();
        let subtitle_str = session.subtitle_text(self.home_dir.as_deref());

        let title_widget = if session.waiting_for_input {
            text::body(title_str).class(theme::Text::Color(Color::from_rgb8(0xf8, 0xe4, 0x5c)))
        } else {
            text::body(title_str)
        };

        let subtitle_widget = text::caption(subtitle_str)
            .class(theme::Text::Color(Color::from_rgb8(0x9a, 0x99, 0x96)));

        let text_col = column::with_capacity(2)
            .spacing(2)
            .width(Length::Fill)
            .push(title_widget)
            .push(subtitle_widget);

        let term_icon = icon::from_name("utilities-terminal-symbolic")
            .symbolic(true)
            .size(16);

        let restore_btn = Self::action_pill_button(
            "Restore",
            Message::RestoreDetached(session.pid),
            PillKind::Restore,
        );
        let kill_btn = Self::action_pill_button(
            "Kill",
            Message::KillDetached(session.pid),
            PillKind::Kill,
        );

        padded_control(
            row::with_capacity(4)
                .align_y(Alignment::Center)
                .spacing(spacing.space_xs)
                .push(term_icon)
                .push(text_col)
                .push(restore_btn)
                .push(kill_btn),
        )
        .into()
    }

    fn view_pending_task_row<'a>(&'a self, task: &'a PendingTaskInfo) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let title_str = task.title_text().to_string();
        let subtitle_str = task.subtitle_text(self.now_unix);

        let title_widget = text::body(title_str);
        let subtitle_widget = text::caption(subtitle_str)
            .class(theme::Text::Color(Color::from_rgb8(0x9a, 0x99, 0x96)));

        let text_col = column::with_capacity(2)
            .spacing(2)
            .width(Length::Fill)
            .push(title_widget)
            .push(subtitle_widget);

        let char_info = self.catalog.iter().find(|c| {
            c.config.id == task.character_id
                || (!task.character_slug.is_empty() && c.config.name == task.character_slug)
        });
        let avatar_or_dot = if let Some(info) = char_info {
            self.view_avatar_or_dot(info)
        } else if let Some(handle) = self.avatar_cache.get(&task.character_slug) {
            icon::icon(handle.clone())
                .width(Length::Fixed(AVATAR_DISPLAY_SIZE))
                .height(Length::Fixed(AVATAR_DISPLAY_SIZE))
                .into()
        } else {
            let dot_color = Color::from_rgb8(0x7B, 0x61, 0xFF);
            let dot = container(widget::space::horizontal().width(Length::Fixed(COLOR_DOT_SIZE)))
                .width(Length::Fixed(COLOR_DOT_SIZE))
                .height(Length::Fixed(COLOR_DOT_SIZE))
                .class(theme::Container::custom(move |_| container::Style {
                    background: Some(Background::Color(dot_color)),
                    border: Border {
                        radius: (COLOR_DOT_SIZE * 0.5).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }));
            container(dot)
                .width(Length::Fixed(AVATAR_DISPLAY_SIZE))
                .height(Length::Fixed(AVATAR_DISPLAY_SIZE))
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .into()
        };

        let (focus_label, focus_msg) = if task.is_detached {
            (
                "Restore",
                Message::RestoreDetached(task.pid.unwrap_or(0)),
            )
        } else {
            ("Focus", Message::FocusCharacter(task.pane_id.clone()))
        };

        let focus_btn = Self::action_pill_button(focus_label, focus_msg, PillKind::Restore);
        let cancel_btn = Self::action_pill_button(
            "Cancel",
            Message::CancelTask(task.pane_id.clone(), task.id.clone()),
            PillKind::Kill,
        );

        padded_control(
            row::with_capacity(4)
                .align_y(Alignment::Center)
                .spacing(spacing.space_xs)
                .push(avatar_or_dot)
                .push(text_col)
                .push(focus_btn)
                .push(cancel_btn),
        )
        .into()
    }

    fn view_character_row<'a>(&'a self, info: &'a CharacterInfo) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let avatar_or_dot = self.view_avatar_or_dot(info);
        let name_label = text::body(info.display_label().to_string());

        let is_active = info.is_active();
        let status_label = if is_active {
            text::caption("In Use").class(theme::Text::Color(Color::from_rgb8(0x33, 0xd1, 0x7a)))
        } else {
            text::caption("Available").class(theme::Text::Color(Color::from_rgb8(0x9a, 0x99, 0x96)))
        };

        let inner_row = row::with_capacity(4)
            .align_y(Alignment::Center)
            .spacing(spacing.space_xs)
            .push(avatar_or_dot)
            .push(name_label)
            .push(widget::space::horizontal().width(Length::Fill))
            .push(status_label);

        if let Some(pane_id) = info.active_pane_id() {
            menu_button(inner_row)
                .on_press(Message::FocusCharacter(pane_id.to_string()))
                .into()
        } else {
            padded_control(inner_row).into()
        }
    }

    fn view_avatar_or_dot<'a>(&'a self, info: &'a CharacterInfo) -> Element<'a, Message> {
        if let Some(handle) = self.avatar_cache.get(&info.config.name) {
            return icon::icon(handle.clone())
                .width(Length::Fixed(AVATAR_DISPLAY_SIZE))
                .height(Length::Fixed(AVATAR_DISPLAY_SIZE))
                .into();
        }

        let (r, g, b) = parse_hex_color(&info.config.color).unwrap_or((0x7B, 0x61, 0xFF));
        let dot_color = Color::from_rgb8(r, g, b);

        let dot = container(widget::space::horizontal().width(Length::Fixed(COLOR_DOT_SIZE)))
            .width(Length::Fixed(COLOR_DOT_SIZE))
            .height(Length::Fixed(COLOR_DOT_SIZE))
            .class(theme::Container::custom(move |_| container::Style {
                background: Some(Background::Color(dot_color)),
                border: Border {
                    radius: (COLOR_DOT_SIZE * 0.5).into(),
                    ..Default::default()
                },
                ..Default::default()
            }));

        container(dot)
            .width(Length::Fixed(AVATAR_DISPLAY_SIZE))
            .height(Length::Fixed(AVATAR_DISPLAY_SIZE))
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
    }

    fn view_shortcuts_popup(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let mut content = column::with_capacity(6)
            .padding([8, 0])
            .spacing(spacing.space_xxs)
            .width(Length::Fill);

        let header = padded_control(
            row::with_capacity(2)
                .align_y(Alignment::Center)
                .spacing(spacing.space_xs)
                .push(
                    button::custom(
                        row::with_capacity(2)
                            .align_y(Alignment::Center)
                            .spacing(4)
                            .push(
                                icon::from_name("go-previous-symbolic")
                                    .symbolic(true)
                                    .size(16),
                            )
                            .push(text::caption("Back")),
                    )
                    .padding([4, 8])
                    .class(theme::Button::Standard)
                    .on_press(Message::CloseShortcutsPage),
                )
                .push(text::body("Character Focus Shortcuts")),
        );
        content = content.push(header);

        let hint = padded_control(
            text::caption(
                "Assign a character and shortcut (e.g. Super+Shift+1). Syncs with COSMIC Shortcuts.",
            )
            .class(theme::Text::Color(Color::from_rgb8(0x9a, 0x99, 0x96))),
        );
        content = content.push(hint);

        content = content.push(
            padded_control(divider::horizontal::default())
                .padding([spacing.space_xxs, spacing.space_s]),
        );

        let mut slots_col = column::with_capacity(SLOT_COUNT)
            .spacing(spacing.space_xs)
            .width(Length::Fill);

        for idx in 0..SLOT_COUNT {
            let slot = &self.config.slots[idx];
            let selected_label = if slot.slug.trim().is_empty() {
                "None".to_string()
            } else {
                self.catalog
                    .iter()
                    .find(|c| c.config.name == slot.slug)
                    .map(|c| c.display_label().to_string())
                    .unwrap_or_else(|| slot.slug.clone())
            };

            let top_row = row::with_capacity(3)
                .align_y(Alignment::Center)
                .spacing(spacing.space_xs)
                .push(text::caption(format!("Slot {}", idx + 1)))
                .push(
                    button::custom(
                        row::with_capacity(2)
                            .align_y(Alignment::Center)
                            .spacing(4)
                            .push(text::caption(selected_label))
                            .push(
                                icon::from_name("pan-down-symbolic")
                                    .symbolic(true)
                                    .size(12),
                            ),
                    )
                    .padding([3, 8])
                    .class(theme::Button::Standard)
                    .on_press(Message::CycleSlotCharacter(idx)),
                )
                .push(widget::space::horizontal().width(Length::Fill))
                .push(
                    button::custom(text::caption("Clear"))
                        .padding([3, 6])
                        .class(theme::Button::Text)
                        .on_press(Message::ClearSlot(idx)),
                );

            let shortcut_input = text_input("e.g. Super+Shift+1", &slot.shortcut)
                .on_input(move |val| Message::SlotShortcutChanged(idx, val))
                .width(Length::Fill);

            let slot_card = padded_control(
                column::with_capacity(2)
                    .spacing(4)
                    .push(top_row)
                    .push(shortcut_input),
            );
            slots_col = slots_col.push(slot_card);
        }

        content = content.push(scrollable(slots_col).height(Length::Fixed(360.0)));
        content.into()
    }

    fn action_pill_button(label: &'static str, msg: Message, kind: PillKind) -> Element<'static, Message> {
        let (bg, fg) = match kind {
            PillKind::Restore => (
                Color::from_rgba8(168, 85, 247, 0.30),
                Color::from_rgb8(0xd8, 0xb4, 0xfe),
            ),
            PillKind::Kill => (
                Color::from_rgba8(224, 27, 36, 0.28),
                Color::from_rgb8(0xf6, 0x61, 0x51),
            ),
        };

        let pill = container(
            text::caption(label).class(theme::Text::Color(fg)),
        )
        .padding([3, 8])
        .class(theme::Container::custom(move |_| container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }));

        button::custom(pill)
            .padding(0)
            .class(theme::Button::Text)
            .on_press(msg)
            .into()
    }
}

#[derive(Debug, Clone, Copy)]
enum PillKind {
    Restore,
    Kill,
}

impl cosmic::Application for BoxxyApplet {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let applet = Self {
            core,
            ..Default::default()
        };
        (applet, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Self::Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subs = vec![
            daemon_subscription().map(Message::DaemonEvent),
            activation_token_subscription(0).map(Message::TokenUpdate),
        ];
        if self.popup.is_some() && !self.pending_tasks.is_empty() {
            subs.push(
                cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::CountdownTick),
            );
        }
        Subscription::batch(subs)
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                    self.popup_page = PopupPage::Main;
                }
            }
            Message::TogglePopup(action, refresh_detached) => {
                self.now_unix = current_unix_secs();
                let surface_task = cosmic::task::message(cosmic::Action::Surface(action));
                if refresh_detached && self.daemon_running {
                    let refresh_detached_task =
                        Task::perform(fetch_detached_sessions_standalone(), |sessions| {
                            cosmic::Action::App(Message::DaemonEvent(
                                DaemonEvent::DetachedUpdated(sessions),
                            ))
                        });
                    let refresh_tasks_task =
                        Task::perform(fetch_pending_tasks_standalone(), |tasks| {
                            cosmic::Action::App(Message::DaemonEvent(
                                DaemonEvent::PendingTasksUpdated(tasks),
                            ))
                        });
                    return Task::batch([surface_task, refresh_detached_task, refresh_tasks_task]);
                }
                return surface_task;
            }
            Message::Surface(action) => {
                return cosmic::task::message(cosmic::Action::Surface(action));
            }
            Message::DaemonEvent(event) => match event {
                DaemonEvent::StatusChanged(running) => {
                    self.daemon_running = running;
                    if !running {
                        self.catalog.clear();
                        self.detached_sessions.clear();
                        self.pending_tasks.clear();
                        self.avatar_cache.clear();
                    }
                }
                DaemonEvent::RegistryUpdated(catalog) => {
                    self.avatar_cache = Self::refresh_avatars_async(catalog.clone());
                    self.catalog = catalog;
                }
                DaemonEvent::DetachedUpdated(sessions) => {
                    self.detached_sessions = sessions;
                }
                DaemonEvent::PendingTasksUpdated(tasks) => {
                    self.now_unix = current_unix_secs();
                    self.pending_tasks = tasks;
                }
            },
            Message::CountdownTick => {
                self.now_unix = current_unix_secs();
            }
            Message::CancelTask(pane_id, task_id) => {
                return Task::perform(
                    async move {
                        cancel_pending_task(&pane_id, &task_id)
                            .await
                            .unwrap_or_default()
                    },
                    |tasks| {
                        cosmic::Action::App(Message::DaemonEvent(
                            DaemonEvent::PendingTasksUpdated(tasks),
                        ))
                    },
                );
            }
            Message::TokenUpdate(update) => match update {
                TokenUpdate::Init(tx) => {
                    self.calloop_token_tx = Some(tx);
                }
                TokenUpdate::Finished => {
                    self.calloop_token_tx = None;
                }
                TokenUpdate::ActivationToken { token, .. } => {
                    present_or_launch_terminal(token.as_deref());
                }
            },
            Message::FocusCharacter(pane_id) => {
                let close_task = self.close_popup_task();
                self.request_terminal_activation();
                let focus_task = Task::perform(
                    async move {
                        if let Err(e) = request_focus_pane(&pane_id).await {
                            log::error!("[boxxy] focus: RequestFocusPane failed: {e}");
                        }
                    },
                    |_| cosmic::Action::App(Message::Noop),
                );
                return Task::batch([close_task, focus_task]);
            }
            Message::RestoreDetached(pid) => {
                let close_task = self.close_popup_task();
                self.request_terminal_activation();
                let restore_task = Task::perform(
                    async move {
                        request_restore_detached(pid)
                            .await
                            .unwrap_or_default()
                    },
                    |sessions| {
                        cosmic::Action::App(Message::DaemonEvent(DaemonEvent::DetachedUpdated(
                            sessions,
                        )))
                    },
                );
                return Task::batch([close_task, restore_task]);
            }
            Message::KillDetached(pid) => {
                return Task::perform(
                    async move {
                        terminate_detached_session(pid)
                            .await
                            .unwrap_or_default()
                    },
                    |sessions| {
                        cosmic::Action::App(Message::DaemonEvent(DaemonEvent::DetachedUpdated(
                            sessions,
                        )))
                    },
                );
            }
            Message::ToggleAgent => {
                let subcommand = if self.daemon_running { "stop" } else { "start" };
                spawn_agent_command(subcommand);
            }
            Message::RestartAgent => {
                spawn_agent_command("restart");
            }
            Message::OpenShortcutsPage => {
                self.popup_page = PopupPage::Shortcuts;
            }
            Message::CloseShortcutsPage => {
                self.popup_page = PopupPage::Main;
            }
            Message::CycleSlotCharacter(idx) => {
                if idx < SLOT_COUNT {
                    let current = self.config.slots[idx].slug.trim();
                    let mut slugs: Vec<String> = vec![String::new()];
                    for c in &self.catalog {
                        slugs.push(c.config.name.clone());
                    }
                    let pos = slugs.iter().position(|s| s == current).unwrap_or(0);
                    let next = (pos + 1) % slugs.len();
                    self.config.slots[idx].slug = slugs[next].clone();
                    if let Err(e) = self.config.save() {
                        log::error!("[boxxy] failed to save shortcut config: {e}");
                    }
                }
            }
            Message::SlotShortcutChanged(idx, shortcut) => {
                if idx < SLOT_COUNT {
                    self.config.slots[idx].shortcut = shortcut;
                    if let Err(e) = self.config.save() {
                        log::error!("[boxxy] failed to save shortcut config: {e}");
                    }
                }
            }
            Message::ClearSlot(idx) => {
                if idx < SLOT_COUNT {
                    self.config.slots[idx].slug.clear();
                    self.config.slots[idx].shortcut.clear();
                    if let Err(e) = self.config.save() {
                        log::error!("[boxxy] failed to save shortcut config: {e}");
                    }
                }
            }
            Message::Noop => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let icon_bytes = if self.daemon_running {
            ICON_ON_BYTES
        } else {
            ICON_OFF_BYTES
        };
        let icon_handle = icon::from_svg_bytes(icon_bytes).symbolic(true);

        let detached_count = if self.daemon_running {
            let detached_without_tasks = self
                .detached_sessions
                .iter()
                .filter(|s| s.pending_task_count == 0)
                .count();
            detached_without_tasks + self.pending_tasks.len()
        } else {
            0
        };
        let any_waiting = self.daemon_running
            && self
                .detached_sessions
                .iter()
                .any(|s| s.waiting_for_input);

        let have_popup = self.popup;
        let daemon_running = self.daemon_running;

        let make_press_handler = move |offset: cosmic::iced::Vector, bounds: Rectangle| {
            if let Some(id) = have_popup {
                Message::Surface(destroy_popup(id))
            } else {
                let surface_action = app_popup::<BoxxyApplet>(
                    |_| Default::default(),
                    move |state: &mut BoxxyApplet| {
                        let new_id = Id::unique();
                        state.popup = Some(new_id);
                        state.popup_page = PopupPage::Main;
                        let mut popup_settings = state.core.applet.get_popup_settings(
                            state.core.main_window_id().unwrap(),
                            new_id,
                            None,
                            None,
                            None,
                        );
                        popup_settings.positioner.anchor_rect = Rectangle {
                            x: (bounds.x - offset.x) as i32,
                            y: (bounds.y - offset.y) as i32,
                            width: bounds.width as i32,
                            height: bounds.height as i32,
                        };
                        popup_settings
                    },
                    Some(Box::new(|state: &BoxxyApplet| {
                        Element::from(state.core.applet.popup_container(state.view_popup()))
                            .map(cosmic::Action::App)
                    })),
                );
                Message::TogglePopup(surface_action, daemon_running)
            }
        };

        let btn: widget::Button<'_, Message> = if detached_count == 0 {
            self.core
                .applet
                .icon_button_from_handle(icon_handle)
                .on_press_with_rectangle(make_press_handler)
        } else {
            let suggested = self.core.applet.suggested_size(true);
            let (pad_major, pad_minor) = self.core.applet.suggested_padding(true);
            let (h_pad, v_pad) = if self.core.applet.is_horizontal() {
                (pad_major, pad_minor)
            } else {
                (pad_minor, pad_major)
            };

            let svg_icon = icon::icon(icon_handle)
                .class(theme::Svg::Custom(std::rc::Rc::new(|theme| {
                    cosmic::iced::widget::svg::Style {
                        color: Some(theme.cosmic().background(theme.transparent).on.into()),
                    }
                })))
                .width(Length::Fixed(suggested.0 as f32))
                .height(Length::Fixed(suggested.1 as f32));

            let (badge_bg, badge_fg) = if any_waiting {
                (
                    Color::from_rgba8(229, 165, 10, 0.92),
                    Color::from_rgb8(0x1e, 0x1e, 0x1e),
                )
            } else {
                (
                    Color::from_rgba8(168, 85, 247, 0.85),
                    Color::from_rgb8(0xff, 0xff, 0xff),
                )
            };

            let badge = container(
                text::caption(format!("{detached_count}")).class(theme::Text::Color(badge_fg)),
            )
            .padding([1, 6])
            .class(theme::Container::custom(move |_| container::Style {
                background: Some(Background::Color(badge_bg)),
                border: Border {
                    radius: 99.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }));

            let inner: Element<'_, Message> = if self.core.applet.is_horizontal() {
                row::with_capacity(2)
                    .align_y(Alignment::Center)
                    .spacing(4)
                    .push(svg_icon)
                    .push(badge)
                    .into()
            } else {
                column::with_capacity(2)
                    .align_x(Alignment::Center)
                    .spacing(2)
                    .push(svg_icon)
                    .push(badge)
                    .into()
            };

            button::custom(inner)
                .padding([v_pad, h_pad])
                .class(theme::Button::AppletIcon)
                .on_press_with_rectangle(make_press_handler)
        };

        let tooltip_wrapped = self.core.applet.applet_tooltip::<Message>(
            btn,
            "Boxxy Terminal",
            self.popup.is_some(),
            Message::Surface,
            None,
        );

        self.core.applet.autosize_window(tooltip_wrapped).into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if self.popup == Some(id) {
            self.core.applet.popup_container(self.view_popup()).into()
        } else {
            text::body("").into()
        }
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

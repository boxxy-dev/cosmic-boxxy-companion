use crate::models::{CharacterInfo, DetachedSessionInfo, PendingTaskInfo, RegistrySnapshot};
use cosmic::iced::Subscription;
use futures::{SinkExt, StreamExt};
use std::any::TypeId;
use std::num::NonZeroUsize;
use zbus::fdo::DBusProxy;
use zbus::names::BusName;
use zbus::{Connection, MatchRule, MessageStream};

pub const D_BUS_SERVICE: &str = "dev.boxxy.BoxxyAgent";
pub const AGENT_PATH: &str = "/dev/boxxy/Agent";
pub const AGENT_IFACE: &str = "dev.boxxy.BoxxyTerminal.Agent";

const METHOD_GET_SNAPSHOT: &str = "GetRegistrySnapshot";
const METHOD_REQUEST_FOCUS: &str = "RequestFocusPane";
const METHOD_GET_DETACHED: &str = "GetDetachedSessions";
const METHOD_RESTORE_DETACHED: &str = "RequestRestoreDetached";
const METHOD_TERMINATE_DETACHED: &str = "TerminateDetachedSession";
const METHOD_GET_PENDING_TASKS: &str = "GetPendingTasks";
const METHOD_CANCEL_PENDING_TASK: &str = "CancelPendingTask";

const SIGNAL_CLAIMS_CHANGED: &str = "ClaimsChanged";
const SIGNAL_DETACHED_CHANGED: &str = "DetachedSessionsChanged";
const SIGNAL_PENDING_TASKS_CHANGED: &str = "PendingTasksChanged";

#[derive(Debug, Clone)]
pub enum DaemonEvent {
    StatusChanged(bool),
    RegistryUpdated(Vec<CharacterInfo>),
    DetachedUpdated(Vec<DetachedSessionInfo>),
    PendingTasksUpdated(Vec<PendingTaskInfo>),
}

struct DaemonSubscriptionTag;

pub fn daemon_subscription() -> Subscription<DaemonEvent> {
    Subscription::run_with(TypeId::of::<DaemonSubscriptionTag>(), |_| {
        cosmic::iced::stream::channel(
            32,
            |mut output: cosmic::iced::futures::channel::mpsc::Sender<DaemonEvent>| async move {
                if let Err(e) = run_daemon_watcher(&mut output).await {
                    log::error!("[boxxy] D-Bus watcher error: {e}");
                }
                std::future::pending::<()>().await;
            },
        )
    })
}

async fn run_daemon_watcher(
    output: &mut cosmic::iced::futures::channel::mpsc::Sender<DaemonEvent>,
) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let dbus_proxy = DBusProxy::new(&conn).await?;

    let service_name: BusName<'_> = D_BUS_SERVICE.try_into()?;
    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, D_BUS_SERVICE)])
        .await?;

    let queue_cap = NonZeroUsize::new(16);

    let claims_rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(D_BUS_SERVICE)
        .path(AGENT_PATH)
        .interface(AGENT_IFACE)
        .member(SIGNAL_CLAIMS_CHANGED)
        .build()?;
    let mut claims_stream = MessageStream::for_match_rule(claims_rule, &conn, queue_cap).await?;

    let detached_rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(D_BUS_SERVICE)
        .path(AGENT_PATH)
        .interface(AGENT_IFACE)
        .member(SIGNAL_DETACHED_CHANGED)
        .build()?;
    let mut detached_stream =
        MessageStream::for_match_rule(detached_rule, &conn, queue_cap).await?;

    let tasks_rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(D_BUS_SERVICE)
        .path(AGENT_PATH)
        .interface(AGENT_IFACE)
        .member(SIGNAL_PENDING_TASKS_CHANGED)
        .build()?;
    let mut tasks_stream = MessageStream::for_match_rule(tasks_rule, &conn, queue_cap).await?;

    let initially_running = dbus_proxy.name_has_owner(service_name.clone()).await.unwrap_or(false);
    let _ = output.send(DaemonEvent::StatusChanged(initially_running)).await;

    if initially_running {
        if let Ok(catalog) = fetch_registry_catalog(&conn).await {
            let _ = output.send(DaemonEvent::RegistryUpdated(catalog)).await;
        }
        if let Ok(detached) = fetch_detached_sessions(&conn).await {
            let _ = output.send(DaemonEvent::DetachedUpdated(detached)).await;
        }
        if let Ok(tasks) = fetch_pending_tasks(&conn).await {
            let _ = output.send(DaemonEvent::PendingTasksUpdated(tasks)).await;
        }
    }

    loop {
        tokio::select! {
            Some(signal) = name_owner_stream.next() => {
                if let Ok(args) = signal.args() {
                    let is_running = args
                        .new_owner()
                        .as_ref()
                        .is_some_and(|owner| !owner.as_str().is_empty());
                    let _ = output.send(DaemonEvent::StatusChanged(is_running)).await;
                    if is_running {
                        if let Ok(catalog) = fetch_registry_catalog(&conn).await {
                            let _ = output.send(DaemonEvent::RegistryUpdated(catalog)).await;
                        }
                        if let Ok(detached) = fetch_detached_sessions(&conn).await {
                            let _ = output.send(DaemonEvent::DetachedUpdated(detached)).await;
                        }
                        if let Ok(tasks) = fetch_pending_tasks(&conn).await {
                            let _ = output.send(DaemonEvent::PendingTasksUpdated(tasks)).await;
                        }
                    }
                }
            }
            Some(Ok(msg)) = claims_stream.next() => {
                let body = msg.body();
                if let Ok(json) = body.deserialize::<String>()
                    && let Ok(snapshot) = serde_json::from_str::<RegistrySnapshot>(&json)
                {
                    let _ = output.send(DaemonEvent::RegistryUpdated(snapshot.catalog)).await;
                }
            }
            Some(Ok(msg)) = detached_stream.next() => {
                let body = msg.body();
                if let Ok(json) = body.deserialize::<String>()
                    && let Ok(sessions) = serde_json::from_str::<Vec<DetachedSessionInfo>>(&json)
                {
                    let _ = output.send(DaemonEvent::DetachedUpdated(sessions)).await;
                }
            }
            Some(Ok(msg)) = tasks_stream.next() => {
                let body = msg.body();
                if let Ok(json) = body.deserialize::<String>()
                    && let Ok(tasks) = serde_json::from_str::<Vec<PendingTaskInfo>>(&json)
                {
                    let _ = output.send(DaemonEvent::PendingTasksUpdated(tasks)).await;
                }
            }
            else => break,
        }
    }

    Ok(())
}

pub async fn is_daemon_running(conn: &Connection) -> bool {
    let Ok(dbus_proxy) = DBusProxy::new(conn).await else {
        return false;
    };
    let Ok(service_name) = D_BUS_SERVICE.try_into() else {
        return false;
    };
    dbus_proxy.name_has_owner(service_name).await.unwrap_or(false)
}

pub async fn fetch_registry_catalog(conn: &Connection) -> zbus::Result<Vec<CharacterInfo>> {
    if !is_daemon_running(conn).await {
        return Ok(Vec::new());
    }
    let reply = conn
        .call_method(
            Some(D_BUS_SERVICE),
            AGENT_PATH,
            Some(AGENT_IFACE),
            METHOD_GET_SNAPSHOT,
            &(),
        )
        .await?;
    let body = reply.body();
    let json: String = body.deserialize()?;
    let snapshot: RegistrySnapshot = serde_json::from_str(&json).unwrap_or_default();
    Ok(snapshot.catalog)
}

pub async fn fetch_detached_sessions(conn: &Connection) -> zbus::Result<Vec<DetachedSessionInfo>> {
    if !is_daemon_running(conn).await {
        return Ok(Vec::new());
    }
    let reply = conn
        .call_method(
            Some(D_BUS_SERVICE),
            AGENT_PATH,
            Some(AGENT_IFACE),
            METHOD_GET_DETACHED,
            &(),
        )
        .await?;
    let body = reply.body();
    let json: String = body.deserialize()?;
    let sessions: Vec<DetachedSessionInfo> = serde_json::from_str(&json).unwrap_or_default();
    Ok(sessions)
}

pub async fn fetch_detached_sessions_standalone() -> Vec<DetachedSessionInfo> {
    let Ok(conn) = Connection::session().await else {
        return Vec::new();
    };
    fetch_detached_sessions(&conn).await.unwrap_or_default()
}

pub async fn fetch_pending_tasks(conn: &Connection) -> zbus::Result<Vec<PendingTaskInfo>> {
    if !is_daemon_running(conn).await {
        return Ok(Vec::new());
    }
    let reply = conn
        .call_method(
            Some(D_BUS_SERVICE),
            AGENT_PATH,
            Some(AGENT_IFACE),
            METHOD_GET_PENDING_TASKS,
            &(),
        )
        .await?;
    let body = reply.body();
    let json: String = body.deserialize()?;
    let tasks: Vec<PendingTaskInfo> = serde_json::from_str(&json).unwrap_or_default();
    Ok(tasks)
}

pub async fn fetch_pending_tasks_standalone() -> Vec<PendingTaskInfo> {
    let Ok(conn) = Connection::session().await else {
        return Vec::new();
    };
    fetch_pending_tasks(&conn).await.unwrap_or_default()
}

pub async fn cancel_pending_task(
    pane_id: &str,
    task_id: &str,
) -> zbus::Result<Vec<PendingTaskInfo>> {
    let conn = Connection::session().await?;
    if !is_daemon_running(&conn).await {
        return Ok(Vec::new());
    }
    conn.call_method(
        Some(D_BUS_SERVICE),
        AGENT_PATH,
        Some(AGENT_IFACE),
        METHOD_CANCEL_PENDING_TASK,
        &(pane_id, task_id),
    )
    .await?;
    fetch_pending_tasks(&conn).await
}

pub async fn request_focus_pane(pane_id: &str) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    if !is_daemon_running(&conn).await {
        return Ok(());
    }
    conn.call_method(
        Some(D_BUS_SERVICE),
        AGENT_PATH,
        Some(AGENT_IFACE),
        METHOD_REQUEST_FOCUS,
        &(pane_id,),
    )
    .await?;
    Ok(())
}

pub async fn request_restore_detached(pid: u32) -> zbus::Result<Vec<DetachedSessionInfo>> {
    let conn = Connection::session().await?;
    if !is_daemon_running(&conn).await {
        return Ok(Vec::new());
    }
    conn.call_method(
        Some(D_BUS_SERVICE),
        AGENT_PATH,
        Some(AGENT_IFACE),
        METHOD_RESTORE_DETACHED,
        &(pid,),
    )
    .await?;
    fetch_detached_sessions(&conn).await
}

pub async fn terminate_detached_session(pid: u32) -> zbus::Result<Vec<DetachedSessionInfo>> {
    let conn = Connection::session().await?;
    if !is_daemon_running(&conn).await {
        return Ok(Vec::new());
    }
    conn.call_method(
        Some(D_BUS_SERVICE),
        AGENT_PATH,
        Some(AGENT_IFACE),
        METHOD_TERMINATE_DETACHED,
        &(pid,),
    )
    .await?;
    fetch_detached_sessions(&conn).await
}

/// Resolves a character by `slug` (or display name) in the live registry snapshot
/// and calls `RequestFocusPane(pane_id)` if the character is currently active.
/// Returns `Ok(true)` if the character was found and active.
pub async fn focus_character_by_slug(slug: &str) -> zbus::Result<bool> {
    let target = slug.trim();
    if target.is_empty() {
        return Ok(false);
    }

    let conn = Connection::session().await?;
    let catalog = fetch_registry_catalog(&conn).await?;

    let Some(char_info) = catalog.iter().find(|c| {
        c.config.name.eq_ignore_ascii_case(target)
            || c.config.display_name.eq_ignore_ascii_case(target)
    }) else {
        log::info!("[boxxy] shortcut: character '{target}' not found in catalog");
        return Ok(false);
    };

    if let Some(pane_id) = char_info.active_pane_id() {
        log::info!(
            "[boxxy] shortcut: focusing character '{}' (pane={})",
            char_info.config.name,
            pane_id
        );
        conn.call_method(
            Some(D_BUS_SERVICE),
            AGENT_PATH,
            Some(AGENT_IFACE),
            METHOD_REQUEST_FOCUS,
            &(pane_id,),
        )
        .await?;
        Ok(true)
    } else {
        log::info!("[boxxy] shortcut: character '{target}' is not active");
        Ok(false)
    }
}

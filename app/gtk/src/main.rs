#[macro_use]
extern crate log;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::process::Command;

use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;

use gtk4::prelude::*;

use gnomeqs_core::channel::{ChannelAction, ChannelDirection, ChannelMessage};
use gnomeqs_core::{RQS, Visibility};
use tokio_util::sync::CancellationToken;

use bridge::{FromUi, ToUi, WifiDirectSendRequest, WifiDirectSessionReady};
use state::AppState;

mod bridge;
mod config;
mod i18n;
mod settings;
mod state;
mod tray_ipc;
mod transfer_history;
mod ui;

fn main() -> anyhow::Result<()> {
    if handle_cli_flags() {
        return Ok(());
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    #[cfg(debug_assertions)]
    {
        unsafe { std::env::set_var("GSETTINGS_SCHEMA_DIR", config::SCHEMA_DIR) };
    }

    gtk4::init()?;
    libadwaita::init()?;

    let language = settings::get_language();
    i18n::init(Some(&language));

    settings::apply_color_scheme();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let visibility = Visibility::from_raw_value(settings::get_visibility_raw() as u64);
    let port = settings::get_port();
    let download_path = settings::get_download_folder();

    let rqs = rt.block_on(async {
        let mut r = RQS::new(visibility, port, download_path);
        r.run().await.map(|(send_info_tx, ble_rx)| (r, send_info_tx, ble_rx))
    })?;

    let (core_rqs, send_info_tx, ble_rx) = rqs;

    let (to_ui_tx, to_ui_rx) = async_channel::bounded::<ToUi>(128);
    let (from_ui_tx, from_ui_rx) = async_channel::unbounded::<FromUi>();

    {
        let mut msg_rx = core_rqs.message_sender.subscribe();
        let tx = to_ui_tx.clone();
        rt.spawn(async move {
            loop {
                match msg_rx.recv().await {
                    Ok(msg) => {
                        if tx.send(ToUi::TransferUpdate(msg)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Message channel lagged by {n}");
                    }
                    Err(_) => break,
                }
            }
        });
    }

    {
        let mut rx = ble_rx;
        let tx = to_ui_tx.clone();
        let vis_sender = Arc::clone(&core_rqs.visibility_sender);
        rt.spawn(async move {
            let mut last_sent = Instant::now() - Duration::from_secs(120);
            loop {
                match rx.recv().await {
                    Ok(_) => {
                        let vis = *vis_sender.lock().unwrap().borrow();
                        if vis == Visibility::Invisible
                            && last_sent.elapsed() >= Duration::from_secs(120)
                        {
                            let _ = tx.send(ToUi::BleNearby).await;
                            last_sent = Instant::now();
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        });
    }

    let tray_handle = tray_ipc::initialize_tray_runtime();

    if let Some(handle) = tray_handle.as_ref() {
        let socket_path = handle.socket_path.clone();
        let from_ui_tx = from_ui_tx.clone();
        rt.spawn(async move {
            let listener = match UnixListener::bind(&socket_path) {
                Ok(listener) => listener,
                Err(e) => {
                    warn!("tray ipc bind failed: {}", e);
                    return;
                }
            };

            loop {
                match listener.accept().await {
                    Ok((mut stream, _)) => {
                        let mut buf = Vec::new();
                        match stream.read_to_end(&mut buf).await {
                            Ok(_) => {
                                if let Ok(cmd) = String::from_utf8(buf) {
                                    tray_ipc::handle_ipc_command(&cmd, &from_ui_tx);
                                }
                            }
                            Err(e) => warn!("tray ipc read failed: {}", e),
                        }
                    }
                    Err(e) => {
                        warn!("tray ipc accept failed: {}", e);
                        break;
                    }
                }
            }
        });
    }

    {
        let mut vis_rx = core_rqs.visibility_sender.lock().unwrap().subscribe();
        let tx = to_ui_tx.clone();
        let tray_handle = tray_handle.clone();
        rt.spawn(async move {
            loop {
                if vis_rx.changed().await.is_err() {
                    break;
                }
                let vis = *vis_rx.borrow_and_update();
                settings::set_visibility_raw(vis as i32);
                if let Some(handle) = tray_handle.as_ref() {
                    handle.set_visibility(vis);
                }
                let _ = tx.send(ToUi::VisibilityChanged(vis)).await;
            }
        });
    }

    let rqs_arc = Arc::new(Mutex::new(core_rqs));
    let discovery_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> =
        Arc::new(Mutex::new(None));
    let wifi_direct_task: Arc<
        Mutex<Option<(CancellationToken, tokio::task::JoinHandle<()>)>>,
    > = Arc::new(Mutex::new(None));
    {
        let rqs = Arc::clone(&rqs_arc);
        let send_tx = send_info_tx.clone();
        let ui_tx = to_ui_tx.clone();
        let discovery_task = Arc::clone(&discovery_task);
        let wifi_direct_task = Arc::clone(&wifi_direct_task);
        rt.spawn(async move {
            while let Ok(cmd) = from_ui_rx.recv().await {
                handle_from_ui(
                    cmd,
                    &rqs,
                    &send_tx,
                    &ui_tx,
                    &discovery_task,
                    &wifi_direct_task,
                )
                .await;
            }
        });
    }

    let app = libadwaita::Application::new(
        Some(config::APP_ID),
        gio::ApplicationFlags::HANDLES_COMMAND_LINE,
    );

    {
        let from_ui_tx = from_ui_tx.clone();
        let to_ui_rx = to_ui_rx.clone();

        app.connect_activate(move |app| {
            if let Some(win) = app.active_window() {
                win.present();
                return;
            }

            register_app_actions(app, from_ui_tx.clone());

            let app_state = AppState {
                from_ui_tx: from_ui_tx.clone(),
                to_ui_rx: to_ui_rx.clone(),
            };
            let window = ui::window::build_window(app, app_state);

            if settings::get_start_minimized() {
                window.set_visible(false);
            } else {
                window.present();
            }
        });
    }

    {
        let tx = to_ui_tx.clone();
        let action = gio::SimpleAction::new("show-receive", None);
        action.connect_activate(move |_, _| {
            let _ = tx.try_send(ToUi::ShowWindowOnPage("receive".to_string()));
        });
        app.add_action(&action);
    }
    {
        let tx = to_ui_tx.clone();
        let action = gio::SimpleAction::new("show-send", None);
        action.connect_activate(move |_, _| {
            let _ = tx.try_send(ToUi::ShowWindowOnPage("send".to_string()));
        });
        app.add_action(&action);
    }
    {
        let tx = to_ui_tx.clone();
        let action = gio::SimpleAction::new("show-settings", None);
        action.connect_activate(move |_, _| {
            let _ = tx.try_send(ToUi::ShowSettings);
        });
        app.add_action(&action);
    }

    {
        let to_ui_tx = to_ui_tx.clone();
        app.connect_command_line(move |app, cmdline| {
            let args = cmdline.arguments();
            let args: Vec<String> = args
                .iter()
                .filter_map(|a| a.to_str().map(|s| s.to_string()))
                .collect();

            let mut page: Option<String> = None;
            let mut iter = args.iter().skip(1);
            while let Some(arg) = iter.next() {
                if arg == "--page" {
                    page = iter.next().cloned();
                } else if let Some(val) = arg.strip_prefix("--page=") {
                    page = Some(val.to_string());
                }
            }

            if let Some(p) = page {
                if p == "settings" {
                    let _ = to_ui_tx.try_send(ToUi::ShowSettings);
                } else {
                    let _ = to_ui_tx.try_send(ToUi::ShowWindowOnPage(p));
                }
            }

            app.activate();
            0
        });
    }

    let _guard = rt.enter();

    let exit_code = app.run();

    drop(_guard);
    rt.block_on(async {
        rqs_arc.lock().unwrap().stop().await;
    });
    if let Some(handle) = tray_handle.as_ref() {
        handle.shutdown();
    }
    rt.shutdown_timeout(Duration::from_secs(5));

    std::process::exit(exit_code.into());
}

fn handle_cli_flags() -> bool {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version") | Some("-V") | Some("-v") => {
            println!("gnomeqs {}", config::VERSION);
            true
        }
        Some("--help") | Some("-h") => {
            println!(
                "GnomeQS {}\n\nUsage:\n  gnomeqs [OPTIONS]\n\nOptions:\n  -h, --help       Show this help message\n  -v, -V, --version    Show version information",
                config::VERSION
            );
            true
        }
        _ => false,
    }
}

fn register_app_actions(app: &libadwaita::Application, from_ui_tx: async_channel::Sender<FromUi>) {
    {
        let tx = from_ui_tx.clone();
        let action = gio::SimpleAction::new("accept-transfer", Some(glib::VariantTy::STRING));
        action.connect_activate(move |_, param| {
            if let Some(id) = param.and_then(|v| v.get::<String>()) {
                if let Err(e) = tx.try_send(FromUi::Accept(id)) {
                    warn!("accept-transfer action: {e}");
                }
            }
        });
        app.add_action(&action);
    }

    {
        let tx = from_ui_tx.clone();
        let action = gio::SimpleAction::new("reject-transfer", Some(glib::VariantTy::STRING));
        action.connect_activate(move |_, param| {
            if let Some(id) = param.and_then(|v| v.get::<String>()) {
                if let Err(e) = tx.try_send(FromUi::Reject(id)) {
                    warn!("reject-transfer action: {e}");
                }
            }
        });
        app.add_action(&action);
    }

    {
        let app = app.clone();
        let action = gio::SimpleAction::new("restart", None);
        let app_for_handler = app.clone();
        action.connect_activate(move |_, _| {
            match std::env::current_exe() {
                Ok(exe) => {
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(350));
                        if let Err(e) = Command::new(exe).spawn() {
                            warn!("restart action failed: {e}");
                        }
                    });
                    app_for_handler.quit();
                }
                Err(e) => warn!("could not resolve current executable for restart: {e}"),
            }
        });
        app.add_action(&action);
    }
}

async fn handle_from_ui(
    cmd: FromUi,
    rqs: &Arc<Mutex<RQS>>,
    send_tx: &tokio::sync::mpsc::Sender<gnomeqs_core::SendInfo>,
    ui_tx: &async_channel::Sender<ToUi>,
    discovery_task: &Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    wifi_direct_task: &Arc<Mutex<Option<(CancellationToken, tokio::task::JoinHandle<()>)>>>,
) {
    match cmd {
        FromUi::Accept(id) => {
            let msg = ChannelMessage {
                id,
                direction: ChannelDirection::FrontToLib,
                action: Some(ChannelAction::AcceptTransfer),
                rtype: None,
                state: None,
                meta: None,
            };
            let _ = rqs.lock().unwrap().message_sender.send(msg);
        }
        FromUi::Reject(id) => {
            let msg = ChannelMessage {
                id,
                direction: ChannelDirection::FrontToLib,
                action: Some(ChannelAction::RejectTransfer),
                rtype: None,
                state: None,
                meta: None,
            };
            let _ = rqs.lock().unwrap().message_sender.send(msg);
        }
        FromUi::Cancel(id) => {
            info!("from_ui cancel received for transfer_id={}", id);
            let msg = ChannelMessage {
                id: id.clone(),
                direction: ChannelDirection::FrontToLib,
                action: Some(ChannelAction::CancelTransfer),
                rtype: None,
                state: None,
                meta: None,
            };
            rqs.lock().unwrap().cancel_transfer(id);
            let _ = rqs.lock().unwrap().message_sender.send(msg);
        }
        FromUi::SendPayload(info) => {
            if let Err(e) = send_tx.send(info).await {
                warn!("SendPayload: {e}");
            }
        }
        FromUi::StartWifiDirectSend(WifiDirectSendRequest {
            peer_id,
            peer_name,
            peer_mac,
            files,
        }) => {
            info!(
                "starting Wi-Fi Direct session for peer_id={} peer_name={} files={}",
                peer_id,
                peer_name,
                files.len()
            );
            if let Err(e) = gnomeqs_core::activate_wifi_direct_peer(&peer_mac).await {
                warn!("StartWifiDirectSend (peer_id={peer_id}): {e}");
                let _ = ui_tx
                    .send(ToUi::Toast(
                        tr!("Could not start a Wi-Fi Direct session for {}.")
                            .replace("{}", &peer_name),
                    ))
                    .await;
                return;
            }

            match gnomeqs_core::wait_for_wifi_direct_session(Duration::from_secs(12)).await {
                Ok(Some(session)) => {
                    let _ = ui_tx
                        .send(ToUi::WifiDirectSessionReady(WifiDirectSessionReady {
                            peer_id: peer_id.clone(),
                            peer_name: peer_name.clone(),
                            session: session.clone(),
                        }))
                        .await;

                    if !session.wifi_connected {
                        let _ = ui_tx
                            .send(ToUi::Toast(tr!(
                                "Wi-Fi Direct started, but your current Wi-Fi connection changed."
                            )))
                            .await;
                    } else if !session.peer_ipv4_candidates.is_empty() {
                        let _ = ui_tx
                            .send(ToUi::Toast(tr!(
                                "Wi-Fi Direct session started and a direct peer link was detected."
                            )))
                            .await;
                    } else if !session.ipv4_addresses.is_empty() {
                        let _ = ui_tx
                            .send(ToUi::Toast(tr!(
                                "Wi-Fi Direct session started. The direct transport handoff is still experimental."
                            )))
                            .await;
                    } else {
                        let _ = ui_tx
                            .send(ToUi::Toast(tr!(
                                "Wi-Fi Direct session started, but no direct IP link is available yet."
                            )))
                            .await;
                    }
                    info!(
                        "Wi-Fi Direct session for peer_id={} connection={:?} ip_interface={:?} ipv4={:?} peer_ipv4={:?} wifi_connected={}",
                        peer_id,
                        session.connection_name,
                        session.ip_interface,
                        session.ipv4_addresses,
                        session.peer_ipv4_candidates,
                        session.wifi_connected
                    );
                }
                Ok(None) => {
                    let _ = ui_tx
                        .send(ToUi::Toast(tr!(
                            "Wi-Fi Direct is not available on this device right now."
                        )))
                        .await;
                }
                Err(e) => {
                    warn!("Wi-Fi Direct session wait failed (peer_id={peer_id}): {e}");
                    let _ = ui_tx
                        .send(ToUi::Toast(tr!(
                            "Wi-Fi Direct started, but the session state could not be verified."
                        )))
                        .await;
                }
            }
        }
        FromUi::StartDiscovery(sender) => {
            if let Err(e) = rqs.lock().unwrap().discovery(sender.clone()) {
                warn!("StartDiscovery: {e}");
            }
            let mut guard = discovery_task.lock().unwrap();
            if let Some(handle) = guard.take() {
                handle.abort();
            }
            let mut rx = sender.subscribe();
            let ui_tx = ui_tx.clone();
            *guard = Some(tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(info) => {
                            if ui_tx.send(ToUi::EndpointUpdate(info)).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Endpoint channel lagged by {n}");
                        }
                        Err(_) => break,
                    }
                }
            }));

            if settings::get_wifi_direct_enabled() {
                let mut wd_guard = wifi_direct_task.lock().unwrap();
                if let Some((token, handle)) = wd_guard.take() {
                    token.cancel();
                    handle.abort();
                }

                let token = CancellationToken::new();
                let sender = sender.clone();
                let token_for_task = token.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) =
                        gnomeqs_core::run_wifi_direct_discovery(sender, token_for_task).await
                    {
                        warn!("Wi-Fi Direct discovery: {e}");
                    }
                });
                *wd_guard = Some((token, handle));
            }
        }
        FromUi::StopDiscovery => {
            rqs.lock().unwrap().stop_discovery();
            if let Some(handle) = discovery_task.lock().unwrap().take() {
                handle.abort();
            }
            let task = wifi_direct_task.lock().unwrap().take();
            if let Some((token, handle)) = task {
                token.cancel();
                let _ = handle.await;
            }
        }
        FromUi::ChangeVisibility(vis) => {
            rqs.lock().unwrap().change_visibility(vis);
        }
        FromUi::ChangeDownloadPath(path) => {
            rqs.lock().unwrap().set_download_path(path);
        }
        FromUi::ShowWindow => {
            let _ = ui_tx.send(ToUi::ShowWindow).await;
        }
        FromUi::Quit => {
            let _ = ui_tx.send(ToUi::Quit).await;
        }
    }
}

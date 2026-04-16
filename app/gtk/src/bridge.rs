use std::path::PathBuf;

use gnomeqs_core::channel::ChannelMessage;
use gnomeqs_core::{EndpointInfo, SendInfo, Visibility, WifiDirectSessionInfo};

#[derive(Debug, Clone)]
pub struct WifiDirectSendRequest {
    pub peer_id: String,
    pub peer_name: String,
    pub peer_mac: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WifiDirectSessionReady {
    pub peer_id: String,
    pub peer_name: String,
    pub session: WifiDirectSessionInfo,
}

#[derive(Debug)]
pub enum ToUi {
    TransferUpdate(ChannelMessage),
    EndpointUpdate(EndpointInfo),
    VisibilityChanged(Visibility),
    BleNearby,
    Toast(String),
    WifiDirectSessionReady(WifiDirectSessionReady),
    ShowWindow,
    ShowWindowOnPage(String),
    ShowSettings,
    Quit,
}

#[derive(Debug)]
pub enum FromUi {
    Accept(String),
    Reject(String),
    Cancel(String),
    SendPayload(SendInfo),
    StartWifiDirectSend(WifiDirectSendRequest),
    StartDiscovery(tokio::sync::broadcast::Sender<EndpointInfo>),
    StopDiscovery,
    ChangeVisibility(Visibility),
    ChangeDownloadPath(Option<PathBuf>),
    ShowWindow,
    Quit,
}

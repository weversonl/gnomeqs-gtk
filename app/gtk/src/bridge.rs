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

/// Messages pushed from Tokio worker threads into the GTK main thread.
#[derive(Debug)]
pub enum ToUi {
    /// A transfer changed state (inbound or outbound).
    TransferUpdate(ChannelMessage),
    /// A nearby endpoint appeared or disappeared.
    EndpointUpdate(EndpointInfo),
    /// The local visibility state changed.
    VisibilityChanged(Visibility),
    /// A BLE nearby signal was received (used to prompt temporary visibility).
    BleNearby,
    /// Show an informational toast message.
    Toast(String),
    /// A Wi-Fi Direct session finished setup and may be usable for direct transport.
    WifiDirectSessionReady(WifiDirectSessionReady),
    /// Present the main window (e.g., from tray "Show" action).
    ShowWindow,
    /// Shut down — stops the GTK main loop.
    Quit,
}

/// Messages sent from GTK signal handlers into the Tokio command handler.
#[derive(Debug)]
pub enum FromUi {
    /// Accept an inbound transfer by id.
    Accept(String),
    /// Reject an inbound transfer by id.
    Reject(String),
    /// Cancel an active transfer by id.
    Cancel(String),
    /// Send files/text to a discovered endpoint.
    SendPayload(SendInfo),
    /// Start a Wi-Fi Direct session for a peer and attempt a direct transport handoff.
    StartWifiDirectSend(WifiDirectSendRequest),
    /// Start mDNS discovery; caller provides the sender for discovered endpoints.
    StartDiscovery(tokio::sync::broadcast::Sender<EndpointInfo>),
    /// Stop mDNS discovery.
    StopDiscovery,
    /// Change local device visibility.
    ChangeVisibility(Visibility),
    /// Update the download directory (None = use system default).
    ChangeDownloadPath(Option<PathBuf>),
    /// Present the main window (e.g., from tray "Show" action).
    ShowWindow,
    /// Quit the application cleanly.
    Quit,
}

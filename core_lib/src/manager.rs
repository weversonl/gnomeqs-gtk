use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc::Receiver;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::channel::{ChannelDirection, ChannelMessage, TransferType};
use crate::errors::AppError;
use crate::hdl::{InboundRequest, OutboundPayload, OutboundRequest, State, Visibility};
use crate::{DeviceType, utils::RemoteDeviceInfo};

const INNER_NAME: &str = "TcpServer";

#[derive(Debug, Deserialize, Serialize)]
pub struct SendInfo {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub addr: String,
    pub ob: OutboundPayload,
}

pub struct TcpServer {
    endpoint_id: [u8; 4],
    tcp_listener: TcpListener,
    sender: Sender<ChannelMessage>,
    cancel_sender: Sender<String>,
    mdns_resend_sender: broadcast::Sender<()>,
    connect_receiver: Receiver<SendInfo>,
    visibility_receiver: watch::Receiver<Visibility>,
}

impl TcpServer {
    pub fn new(
        endpoint_id: [u8; 4],
        tcp_listener: TcpListener,
        sender: Sender<ChannelMessage>,
        cancel_sender: Sender<String>,
        mdns_resend_sender: broadcast::Sender<()>,
        connect_receiver: Receiver<SendInfo>,
        visibility_receiver: watch::Receiver<Visibility>,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            endpoint_id,
            tcp_listener,
            sender,
            cancel_sender,
            mdns_resend_sender,
            connect_receiver,
            visibility_receiver,
        })
    }

    pub async fn run(&mut self, ctk: CancellationToken) -> Result<(), anyhow::Error> {
        info!("{INNER_NAME}: service starting");

        loop {
            let cctk = ctk.clone();

            tokio::select! {
                _ = ctk.cancelled() => {
                    info!("{INNER_NAME}: tracker cancelled, breaking");
                    break;
                }
                Some(i) = self.connect_receiver.recv() => {
                    info!("{INNER_NAME}: outbound request: id={} name={} addr={}", i.id, i.name, i.addr);
                    let endpoint_id = self.endpoint_id;
                    let sender = self.sender.clone();
                    let cancel_sender = self.cancel_sender.clone();
                    tokio::spawn(async move {
                        if let Err(e) = connect(cctk, endpoint_id, sender, cancel_sender, i).await {
                            error!("{INNER_NAME}: error sending: {}", e.to_string());
                        }
                    });
                }
                r = self.tcp_listener.accept() => {
                    match r {
                        Ok((socket, remote_addr)) => {
                            trace!("{INNER_NAME}: new client: {remote_addr}");
                            if *self.visibility_receiver.borrow() == Visibility::Invisible {
                                debug!("{INNER_NAME}: rejecting inbound client while hidden");
                                drop(socket);
                                continue;
                            }
                            let esender = self.sender.clone();
                            let csender = self.sender.clone();
                            let mdns_resend_sender = self.mdns_resend_sender.clone();

                            tokio::spawn(async move {
                                let mut ir = InboundRequest::new(socket, remote_addr.to_string(), csender);

                                loop {
                                    match ir.handle().await {
                                        Ok(_) => {},
                                        Err(e) => match e.downcast_ref() {
                                            Some(AppError::NotAnError) => break,
                                            None => {
                                                if ir.state.state == State::Initial {
                                                    break;
                                                }

                                                if ir.state.state != State::Finished {
                                                    ir.cleanup_partial_files();
                                                }

                                                if ir.state.state != State::Finished {
                                                    let _ = esender.send(ChannelMessage {
                                                        id: remote_addr.to_string(),
                                                        direction: ChannelDirection::LibToFront,
                                                        state: Some(State::Disconnected),
                                                        ..Default::default()
                                                    });
                                                }
                                                error!("{INNER_NAME}: error while handling client: {e} ({:?})", ir.state.state);
                                                break;
                                            }
                                        },
                                    }
                                }

                                if ir.state.state != State::Initial {
                                    schedule_mdns_resend(mdns_resend_sender).await;
                                }
                            });
                        },
                        Err(err) => {
                            error!("{INNER_NAME}: error accepting: {}", err);
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

}

async fn connect(
    ctk: CancellationToken,
    endpoint_id: [u8; 4],
    sender: Sender<ChannelMessage>,
    cancel_sender: Sender<String>,
    si: SendInfo,
) -> Result<(), anyhow::Error> {
    debug!("{INNER_NAME}: Connecting to: {}", si.addr);
    let socket = TcpStream::connect(si.addr.clone()).await?;

    let mut or = OutboundRequest::new(
        endpoint_id,
        socket,
        si.id,
        sender.clone(),
        cancel_sender.subscribe(),
        si.ob,
        RemoteDeviceInfo {
            device_type: si.device_type,
            name: si.name,
        },
    );

    or.send_connection_request().await?;
    or.send_ukey2_client_init().await?;

    loop {
        tokio::select! {
            _ = ctk.cancelled() => {
                info!("{INNER_NAME}: tracker cancelled, breaking");
                break;
            },
            r = or.handle() => {
                if let Err(e) = r {
                    match e.downcast_ref() {
                        Some(AppError::NotAnError) => break,
                        None => {
                            if or.state.state == State::Initial {
                                break;
                            }

                            if or.state.state != State::Finished && or.state.state != State::Cancelled {
                                let _ = sender.send(ChannelMessage {
                                    id: or.state.id.clone(),
                                    direction: ChannelDirection::LibToFront,
                                    rtype: Some(TransferType::Outbound),
                                    state: Some(State::Disconnected),
                                    meta: or.state.transfer_metadata.clone(),
                                    ..Default::default()
                                });
                            }
                            error!("{INNER_NAME}: error while handling client: {e} ({:?})", or.state.state);
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn schedule_mdns_resend(sender: broadcast::Sender<()>) {
    // Three staggered re-announcements so Samsung's browse window catches at least one.
    // Each register() call also auto-schedules a second announcement at +1 s (RFC 6762 §8.3),
    // giving ~6 multicast packets total over a 7 s window.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let _ = sender.send(());
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
    let _ = sender.send(());
    tokio::time::sleep(std::time::Duration::from_millis(4000)).await;
    let _ = sender.send(());
}

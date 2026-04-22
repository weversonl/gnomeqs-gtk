use std::sync::{Arc, Mutex};
use std::time::Duration;

use mdns_sd::{AddrType, ServiceDaemon, ServiceInfo};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use tokio::sync::watch;
use tokio::time::{Instant, interval_at};
use tokio_util::sync::CancellationToken;

use crate::utils::{DeviceType, gen_mdns_endpoint_info, gen_mdns_name};

const INNER_NAME: &str = "MDnsServer";
const TICK_INTERVAL: Duration = Duration::from_secs(60);
const RESEND_INTERVAL: Duration = Duration::from_secs(12);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Visibility {
    Visible = 0,
    Invisible = 1,
    Temporarily = 2,
}

#[allow(dead_code)]
impl Visibility {
    pub fn from_raw_value(value: u64) -> Self {
        match value {
            0 => Visibility::Visible,
            1 => Visibility::Invisible,
            2 => Visibility::Temporarily,
            _ => unreachable!(),
        }
    }
}

pub struct MDnsServer {
    daemon: ServiceDaemon,
    service_info: ServiceInfo,
    ble_receiver: Receiver<()>,
    reset_receiver: Receiver<()>,
    visibility_sender: Arc<Mutex<watch::Sender<Visibility>>>,
    visibility_receiver: watch::Receiver<Visibility>,
}

impl MDnsServer {
    pub fn new(
        endpoint_id: [u8; 4],
        service_port: u16,
        ble_receiver: Receiver<()>,
        reset_receiver: Receiver<()>,
        visibility_sender: Arc<Mutex<watch::Sender<Visibility>>>,
        visibility_receiver: watch::Receiver<Visibility>,
    ) -> Result<Self, anyhow::Error> {
        let service_info = Self::build_service(endpoint_id, service_port, DeviceType::Laptop)?;

        Ok(Self {
            daemon: ServiceDaemon::new()?,
            service_info,
            ble_receiver,
            reset_receiver,
            visibility_sender,
            visibility_receiver,
        })
    }

    pub async fn run(&mut self, ctk: CancellationToken) -> Result<(), anyhow::Error> {
        info!("{INNER_NAME}: service starting");
        let monitor = self.daemon.monitor()?;
        let ble_receiver = &mut self.ble_receiver;
        let reset_receiver = &mut self.reset_receiver;
        let mut visibility = *self.visibility_receiver.borrow();
        let mut temporary_visibility_interval =
            interval_at(Instant::now() + TICK_INTERVAL, TICK_INTERVAL);
        let mut resend_interval = interval_at(Instant::now() + RESEND_INTERVAL, RESEND_INTERVAL);
        let mut registered = false;

        if visibility != Visibility::Invisible {
            self.daemon.register(self.service_info.clone())?;
            registered = true;
        }

        loop {
            tokio::select! {
                _ = ctk.cancelled() => {
                    info!("{INNER_NAME}: tracker cancelled, breaking");
                    break;
                }
                r = monitor.recv_async() => {
                    match r {
                        Ok(_) => continue,
                        Err(err) => return Err(err.into()),
                    }
                },
                _ = self.visibility_receiver.changed() => {
                    visibility = *self.visibility_receiver.borrow_and_update();

                    debug!("{INNER_NAME}: visibility changed: {visibility:?}");
                    if visibility == Visibility::Visible {
                        if !registered {
                            self.daemon.register(self.service_info.clone())?;
                            registered = true;
                        }
                    } else if visibility == Visibility::Invisible {
                        if registered {
                            let receiver = self.daemon.unregister(self.service_info.get_fullname())?;
                            let _ = receiver.recv();
                            registered = false;
                        }
                    } else if visibility == Visibility::Temporarily {
                        if !registered {
                            self.daemon.register(self.service_info.clone())?;
                            registered = true;
                        }
                        temporary_visibility_interval.reset();
                    }
                }
                _ = ble_receiver.recv() => {
                    if visibility == Visibility::Invisible {
                        continue;
                    }

                    debug!("{INNER_NAME}: ble_receiver: got event");
                    if registered {
                        self.daemon.register_resend(self.service_info.get_fullname())?;
                    } else {
                        self.daemon.register(self.service_info.clone())?;
                        registered = true;
                    }
                },
                _ = resend_interval.tick() => {
                    if visibility == Visibility::Invisible {
                        continue;
                    }

                    if registered {
                        debug!("{INNER_NAME}: resending visible service announcement");
                        self.daemon.register_resend(self.service_info.get_fullname())?;
                    } else {
                        self.daemon.register(self.service_info.clone())?;
                        registered = true;
                    }
                },
                _ = reset_receiver.recv() => {
                    if visibility == Visibility::Invisible {
                        continue;
                    }

                    // Re-announce without sending a goodbye first. mdns-sd supports calling
                    // register() again on an already-registered service to re-broadcast it.
                    // Staying registered means Samsung's browse query always gets a response,
                    // even if it fires immediately after the transfer.
                    debug!("{INNER_NAME}: post-transfer reset: re-announcing service");
                    self.daemon.register(self.service_info.clone())?;
                    registered = true;
                },
                _ = temporary_visibility_interval.tick() => {
                    if visibility != Visibility::Temporarily {
                        continue;
                    }

                    if registered {
                        let receiver = self.daemon.unregister(self.service_info.get_fullname())?;
                        let _ = receiver.recv();
                        registered = false;
                    }
                    let _ = self.visibility_sender.lock().unwrap().send(Visibility::Invisible);
                }
            }
        }

        if registered {
            let receiver = self.daemon.unregister(self.service_info.get_fullname())?;
            if let Ok(event) = receiver.recv() {
                info!("MDnsServer: service unregistered: {:?}", &event);
            }
        }

        Ok(())
    }

    fn build_service(
        endpoint_id: [u8; 4],
        service_port: u16,
        device_type: DeviceType,
    ) -> Result<ServiceInfo, anyhow::Error> {
        let name = gen_mdns_name(endpoint_id);
        let hostname = gethostname::gethostname().to_string_lossy().into_owned();
        info!("Broadcasting with: {hostname}");
        let endpoint_info = gen_mdns_endpoint_info(device_type as u8, &hostname);

        let properties = [("n", endpoint_info)];
        let si = ServiceInfo::new(
            "_FC9F5ED42C8A._tcp.local.",
            &name,
            &hostname,
            "",
            service_port,
            &properties[..],
        )?
        .enable_addr_auto(AddrType::V4);

        Ok(si)
    }
}

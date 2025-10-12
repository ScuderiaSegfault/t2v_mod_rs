use std::collections::HashSet;
use std::os::unix::net::SocketAddr;
use async_std::os::unix::net::UnixDatagram;
use async_std::stream::{Stream, StreamExt};
use log::{debug, error, info, warn};
use std::process::exit;
use futures::channel::mpsc::{channel, Sender};
use futures::future::Either;
use futures::{pin_mut, SinkExt};
use crate::StatusNotifier;
use t2v_module::{Initial, IrNecFrame, T2VModule};
use tokio::task;
use t2v_driver_proto::{Event, Request};

pub async fn unit<S: Stream<Item = T2VModule<Initial>> + Unpin>(
    mut devices: S,
    socket: &UnixDatagram,
    notifier: &dyn StatusNotifier,
) -> crate::Result<()> {
    let device = devices.next().await.expect("no device found");
    notifier.ready();
    notifier.report_status("Ready".into());

    let (sender, mut receiver) = channel(16);

    let ir_nec_task = task::spawn(handle_device(device, sender.clone()));
    pin_mut!(ir_nec_task);

    let mut ir_nec_subscribers = HashSet::new();

    loop {
        let handle_incoming = handle_incoming(socket);
        pin_mut!(handle_incoming);

        let receive_frame = receiver.next();
        pin_mut!(receive_frame);


        match futures::future::select(
            futures::future::select(handle_incoming, receive_frame),
            &mut ir_nec_task
        ).await {
            Either::Left((Either::Left((request, _)) , _)) => {
                debug!("incoming request: {:?}", request);
                match request {
                    Ok(Some((request, addr))) => match request {
                        Request::ReceiveNecFrames { enable } => {
                            if let Some(path) = addr.as_pathname() && enable {
                                ir_nec_subscribers.insert(path.to_path_buf());
                            }
                        }
                    }
                    Ok(None) => (),
                    Err(e) => {
                        error!("error for incoming request: {:}", e);
                    }
                }
            }
            Either::Left((Either::Right((frame, _)) , _)) => {
                if !ir_nec_subscribers.is_empty() {
                    let data = serde_json::to_vec(&Event::IrNecFrame(frame.unwrap())).unwrap();
                    for ir_nec_subscriber in &ir_nec_subscribers {
                        debug!("sending event to: {}", ir_nec_subscriber.display());
                        if let Err(e) = socket.send_to(&data, ir_nec_subscriber).await {
                            warn!("error sending event to {}: {e}", ir_nec_subscriber.display());
                        };
                    }
                }
            }
            Either::Right((_, _)) => {
                warn!("device closed, trying next one");
                notifier.report_status("Reconnecting".into());
                let data = serde_json::to_vec(&Event::Disconnected).unwrap();
                for ir_nec_subscriber in &ir_nec_subscribers {
                    debug!("sending event to: {}", ir_nec_subscriber.display());
                    if let Err(e) = socket.send_to(&data, ir_nec_subscriber).await {
                        warn!("error sending event to {}: {e}", ir_nec_subscriber.display());
                    };
                }
                let device = devices.next().await.expect("no device found");
                notifier.report_status("Connected".into());
                let data = serde_json::to_vec(&Event::Connected).unwrap();
                for ir_nec_subscriber in &ir_nec_subscribers {
                    debug!("sending event to: {}", ir_nec_subscriber.display());
                    if let Err(e) = socket.send_to(&data, ir_nec_subscriber).await {
                        warn!("error sending event to {}: {e}", ir_nec_subscriber.display());
                    };
                }
                *ir_nec_task = task::spawn(handle_device(device, sender.clone()));
            }
        }
    }
}

async fn handle_incoming(socket: &UnixDatagram) -> crate::Result<Option<(Request, SocketAddr)>> {
    let mut buffer = vec![0u8; 1024];
    let (bytes, addr) = socket.recv_from(&mut buffer).await?;
    debug!("received {} bytes from {:?}", bytes, addr);

    match serde_json::from_slice::<Request>(&buffer[0..bytes]) {
        Ok(request) => {
            Ok(Some((request, addr)))
        },
        Err(e) => {
            error!("failed to deserialize request: {:?}", e);
            Ok(None)
        }
    }
}

pub async fn handle_device(device: T2VModule<Initial>, mut sender: Sender<IrNecFrame>) -> crate::Result<()> {
    match (device.manufacturer_string(), device.product_string()) {
        (Some(manufacturer_string), Some(product_string)) => {
            info!("found device `{product_string}` from `{manufacturer_string}`");
        }
        (Some(manufacturer_string), None) => {
            info!("found device from `{manufacturer_string}`");
        }
        (None, Some(product_string)) => {
            info!("found device `{product_string}`");
        }
        (None, None) => {
            warn!("found device with no manufacturer and no product string");
        }
    }

    debug!("interfaces:");
    for interface in device.interfaces() {
        debug!("{interface:?}");
    }

    debug!("opening device");
    let device = match device.open().await {
        Ok(device) => device,
        Err(e) => {
            error!("Failed to open device: {e}");
            exit(-1);
        }
    };

    debug!("claiming interface");
    let device = match device.claim().await {
        Ok(device) => device,
        Err(e) => {
            error!("Failed to claim interface: {e}");
            exit(-1);
        }
    };

    let mut ir_nec_reader = device.ir_nec_endpoint()?;

    loop {
        match ir_nec_reader.next().await {
            Ok(option) => match option {
                None => {
                    debug!("End of received messages");
                    return Ok(());
                }
                Some(frame) => {
                    info!("Received frame {frame:02x?}");
                    if let Err(e) = sender.send(frame).await {
                        error!("failed to send frame: {e}");
                    }
                }
            },
            Err(e) => {
                error!("Error while receiving messages {e:?}");
                return Err(e.into());
            }
        }
    }
}

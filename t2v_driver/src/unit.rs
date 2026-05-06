use crate::StatusNotifier;
use async_std::os::unix::net::UnixDatagram;
use async_std::stream::{Stream, StreamExt};
use futures::channel::mpsc::{Sender, channel};
use futures::{SinkExt, pin_mut};
use log::{debug, error, info, warn};
use std::collections::HashSet;
use std::os::unix::net::SocketAddr;
use t2v_driver_proto::{Event, Request};
use t2v_module::{Initial, IrNecFrame, T2VModule};
use tokio::task;
use t2v_driver_proto::{Event as T2VEvent, Request};
use tokio::{select, task};

pub async fn unit<S: Stream<Item = T2VModule<Initial>> + Unpin>(
    mut devices: S,
    socket: &UnixDatagram,
    notifier: &dyn StatusNotifier,
) -> crate::Result<()> {
    let device = devices.next().await.expect("no device found");
    notifier.ready();
    notifier.report_status("Ready".into());

    let (ir_sender, mut ir_receiver) = channel(16);
    let (sensors_sender, mut sensors_receiver) = channel(16);

    let ir_nec_task = task::spawn(handle_device(device, sender.clone()));
    let ir_nec_task = task::spawn(handle_device(
        device,
        ir_sender.clone(),
    ));
    pin_mut!(ir_nec_task);

    let mut ir_nec_subscribers = HashSet::new();

    loop {
        let handle_incoming = handle_incoming(socket);
        pin_mut!(handle_incoming);
        enum Event {
            IncomingPacket(crate::Result<Option<(Request, SocketAddr)>>),
            IrFrame(Option<IrNecFrame>),
            TaskExit(crate::Result<()>),
        }

        let receive_frame = receiver.next();
        pin_mut!(receive_frame);
        let event = select! {
            incoming = handle_incoming(socket) => Event::IncomingPacket(incoming),
            ir_frame = ir_receiver.next() => Event::IrFrame(ir_frame),
            task_exit = &mut ir_nec_task => Event::TaskExit(task_exit?),
        };

        match event {
            Event::IncomingPacket(request) => {
                debug!("incoming request: {:?}", request);
                match request {
                    Ok(Some((request, addr))) => match request {
                        Request::ReceiveNecFrames { enable } => {
                            if let Some(path) = addr.as_pathname()
                                && enable
                            {
                                ir_nec_subscribers.insert(path.to_path_buf());
                            }
                        }
                    },
                    Ok(None) => (),
                    Err(e) => {
                        error!("error for incoming request: {:}", e);
                    }
                }
            }
            Event::IrFrame(frame) => {
                if !ir_nec_subscribers.is_empty() {
                    let data = serde_json::to_vec(&T2VEvent::IrNecFrame(frame.unwrap())).unwrap();
                    for ir_nec_subscriber in &ir_nec_subscribers {
                        debug!("sending event to: {}", ir_nec_subscriber.display());
                        if let Err(e) = socket.send_to(&data, ir_nec_subscriber).await {
                            warn!(
                                "error sending event to {}: {e}",
                                ir_nec_subscriber.display()
                            );
                        };
                    }
                }
            }
            Either::Right((_, _)) => {
            Event::TaskExit(_result) => {
                warn!("device closed, trying next one");
                notifier.report_status("Reconnecting".into());
                let data = serde_json::to_vec(&T2VEvent::Disconnected).unwrap();
                for ir_nec_subscriber in &ir_nec_subscribers {
                    debug!("sending event to: {}", ir_nec_subscriber.display());
                    if let Err(e) = socket.send_to(&data, ir_nec_subscriber).await {
                        warn!(
                            "error sending event to {}: {e}",
                            ir_nec_subscriber.display()
                        );
                    };
                }
                let device = devices.next().await.expect("no device found");
                notifier.report_status("Connected".into());
                let data = serde_json::to_vec(&T2VEvent::Connected).unwrap();
                for ir_nec_subscriber in &ir_nec_subscribers {
                    debug!("sending event to: {}", ir_nec_subscriber.display());
                    if let Err(e) = socket.send_to(&data, ir_nec_subscriber).await {
                        warn!(
                            "error sending event to {}: {e}",
                            ir_nec_subscriber.display()
                        );
                    };
                }
                *ir_nec_task = task::spawn(handle_device(device, sender.clone()));
                *ir_nec_task = task::spawn(handle_device(
                    device,
                    ir_sender.clone(),
                ));
            }
        }
    }
}

async fn handle_incoming(socket: &UnixDatagram) -> crate::Result<Option<(Request, SocketAddr)>> {
    let mut buffer = vec![0u8; 1024];
    let (bytes, addr) = socket.recv_from(&mut buffer).await?;
    debug!("received {} bytes from {:?}", bytes, addr);

    match serde_json::from_slice::<Request>(&buffer[0..bytes]) {
        Ok(request) => Ok(Some((request, addr))),
        Err(e) => {
            error!("failed to deserialize request: {:?}", e);
            Ok(None)
        }
    }
}

pub async fn handle_device(
    device: T2VModule<Initial>,
    mut ir_sender: Sender<IrNecFrame>,
    mut sensors_sender: Sender<TireHallSensorReading>,
) -> crate::Result<()> {
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
            return Err(e.into());
        }
    };

    debug!("claiming interface");
    let device = match device.claim().await {
        Ok(device) => device,
        Err(e) => {
            error!("Failed to claim interface: {e}");
            return Err(e.into());
        }
    };

    let mut ir_nec_reader = device.ir_nec_endpoint()?;
    let mut ir_finished = false;

    loop {
        match ir_nec_reader.next().await {
            Ok(option) => match option {
        enum Event {
            Ir(Result<Option<IrNecFrame>, tokio::io::Error>),
        }
        let event = select! {
            ir_frame = ir_nec_reader.next() => Event::Ir(ir_frame),
        };

        match event {
            Event::Ir(Ok(option)) => match option {
                None => {
                    debug!("End of received messages");
                    return Ok(());
                    debug!("End of IR frames");
                }
                Some(frame) => {
                    info!("Received frame {frame:02x?}");
                    if let Err(e) = ir_sender.send(frame).await {
                        error!("failed to send frame: {e}");
                    }
                }
            },
            Err(e) => {
                error!("Error while receiving messages {e:?}");
            Event::Ir(Err(e)) => {
                error!("Error while receiving IR frames: {e:?}");
                return Err(e.into());
            }
                return Err(e.into());
            }
        }
    }
}

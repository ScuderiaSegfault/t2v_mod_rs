use clap::Parser;
use log::{debug, error, info};
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::process::exit;
use t2v_driver_proto::{Event, Request};
use tempfile::{Builder, NamedTempFile};

#[derive(Debug, Parser)]
struct Args {
    driver_socket: PathBuf,
}

fn main() {
    pretty_env_logger::init_timed();
    let args = Args::parse();
    debug!("Args: {:?}", args);

    info!("Connecting to {}", args.driver_socket.display(),);
    let socket = match Builder::new().make(|path| {
        debug!("Attempting to bind to {}", path.display());
        UnixDatagram::bind(path)
    }) {
        Ok(socket) => socket,
        Err(e) => {
            error!("Failed to create temporary socket: {e}",);
            exit(1);
        }
    };
    let app = App {
        socket,
        driver_socket: args.driver_socket,
    };
    app.run();
}

struct App {
    socket: NamedTempFile<UnixDatagram>,
    driver_socket: PathBuf,
}

impl App {
    fn run(self) {
        info!(
            "Sending `ReceiveNecFrames` request to {}",
            self.driver_socket.display()
        );
        let request = serde_json::to_vec(&Request::ReceiveNecFrames { enable: true }).unwrap();
        self.socket
            .as_file()
            .send_to(request.as_slice(), &self.driver_socket)
            .unwrap();

        debug!("Starting to receive events from driver");
        let mut buffer = vec![0; 1024];
        loop {
            debug!("Waiting for next event");
            let size = self.socket.as_file().recv(&mut buffer).unwrap();
            let event: Event = serde_json::from_slice(&buffer[..size]).unwrap();
            debug!("{:?}", event);

            match event {
                Event::IrNecFrame(frame) => {
                    println!(
                        "NEC Frame: Address={:02x?}, Command={:02x?}",
                        frame.address(),
                        frame.command()
                    );
                }
                Event::Connected => {
                    info!("Connected");
                }
                Event::Disconnected => {
                    info!("Disconnected");
                }
            }
        }
    }
}

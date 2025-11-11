use clap::Parser;
use log::debug;
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use t2v_driver_proto::{Event, Request};

#[derive(Debug, Parser)]
struct Args {
    #[clap(long, default_value = "/tmp/t2v-client")]
    socket: PathBuf,
    driver_socket: PathBuf,
}

fn main() {
    pretty_env_logger::init_timed();
    let args = Args::parse();
    debug!("Args: {:?}", args);

    if args.socket.exists() {
        std::fs::remove_file(&args.socket).unwrap();
    }

    let socket = UnixDatagram::bind(&args.socket).unwrap();
    let app = App {
        socket,
        driver_socket: args.driver_socket,
    };
    app.run();
}

struct App {
    socket: UnixDatagram,
    driver_socket: PathBuf,
}

impl App {
    fn run(self) {
        let request = serde_json::to_vec(&Request::ReceiveNecFrames { enable: true }).unwrap();
        self.socket
            .send_to(request.as_slice(), &self.driver_socket)
            .unwrap();

        let mut buffer = vec![0; 1024];
        loop {
            let size = self.socket.recv(&mut buffer).unwrap();
            let event: Event = serde_json::from_slice(&buffer[..size]).unwrap();

            debug!("{:?}", event);
        }
    }
}

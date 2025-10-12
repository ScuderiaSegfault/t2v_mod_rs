#![feature(let_chains)]//#![feature(let_chains)]

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![forbid(clippy::unwrap_used)]
#![forbid(clippy::expect_used)]
mod error;

use std::borrow::Cow;
use std::fs;

pub use error::{Error, Result};
use std::os::linux::fs::MetadataExt;
use std::path::PathBuf;
mod unit;

use async_std::os::unix::net::UnixDatagram;

use clap::Parser;
use log::{debug, error, info, warn};
use std::process::exit;
use t2v_module::T2VModule;

#[cfg(feature = "systemd")]
use log::LevelFilter;
#[cfg(feature = "systemd")]
use sd_notify::NotifyState;
#[cfg(feature = "systemd")]
use std::os::fd::FromRawFd;
#[cfg(feature = "systemd")]
use systemd_journal_logger::JournalLog;

#[derive(Parser, Debug)]
struct Args {
    #[cfg(feature = "systemd")]
    #[clap(long, env)]
    systemd: bool,
    #[cfg(feature = "systemd")]
    #[clap(short, long, env)]
    socket_file: Option<PathBuf>,
    #[cfg(not(feature = "systemd"))]
    #[clap(short, long, env)]
    socket_file: PathBuf,
    #[clap(long, env, default_value_t = 0x5455)]
    vendor_id: u16,
    #[clap(long, env, default_value_t = 0x1911)]
    product_id: u16,
}

trait StatusNotifier: Send + Sync {
    fn ready(&self);
    fn report_status(&self, status: Cow<'static, str>);
}
#[cfg(feature = "systemd")]
pub struct SystemdNotifier;
pub struct StderrNotifier;
#[cfg(feature = "systemd")]
impl StatusNotifier for SystemdNotifier {
    fn ready(&self) {
        sd_notify::notify(false, &[NotifyState::Ready]).unwrap()
    }

    fn report_status(&self, status: Cow<'static, str>) {
        sd_notify::notify(false, &[NotifyState::Status(status.as_ref())]).unwrap();
    }
}

impl StatusNotifier for StderrNotifier {
    fn ready(&self) {
        info!("unit reports: ready")
    }

    fn report_status(&self, status: Cow<'static, str>) {
        info!("unit reports: status=\"{}\"", status)
    }
}

#[cfg(not(feature = "systemd"))]
async fn setup_unit(args: &Args) -> (UnixDatagram, Box<dyn StatusNotifier>) {
    if args.socket_file.exists() {
        let metadata = args
            .socket_file
            .metadata()
            .expect("load metadata for file that exists");
        let st_mode = metadata.st_mode();
        if st_mode & 0o0170000 == 0o0140000 {
            fs::remove_file(&args.socket_file).expect("remove pre-existing socket file");
        }
    }

    pretty_env_logger::init_timed();
    (
        UnixDatagram::bind(args.socket_file.as_path())
            .await
            .expect("bind socket to socket file"),
        Box::new(StderrNotifier),
    )
}

#[cfg(feature = "systemd")]
async fn setup_unit(args: &Args) -> (UnixDatagram, Box<dyn StatusNotifier>) {
    if args.systemd
        && sd_notify::notify(false, &[NotifyState::Status("Probing for systemd")]).is_ok()
        && systemd_journal_logger::connected_to_journal()
    {
        JournalLog::new().unwrap().install().unwrap();
        log::set_max_level(LevelFilter::Info);
        let raw_fd = sd_notify::listen_fds().unwrap().next().unwrap();
        (
            unsafe { UnixDatagram::from_raw_fd(raw_fd) },
            Box::new(SystemdNotifier) as Box<dyn StatusNotifier>,
        )
    } else {
        eprintln!("systemd support requested but not detected, reverting to normal operation");
        if let Some(socket_file) = &args.socket_file {
            if socket_file.exists() {
                let metadata = socket_file
                    .metadata()
                    .expect("load metadata for file that exists");
                let st_mode = metadata.st_mode();
                if st_mode & 0o0170000 == 0o0140000 {
                    fs::remove_file(socket_file).expect("remove pre-existing socket file");
                }
            }

            pretty_env_logger::init_timed();
            (
                UnixDatagram::bind(socket_file.as_path())
                    .await
                    .expect("bind socket to socket file"),
                Box::new(StderrNotifier),
            )
        } else {
            eprintln!("no socket file provided, exiting");
            exit(1)
        }
    }
}

#[async_std::main]
async fn main() {
    let args = Args::parse();

    let (socket, notifier) = setup_unit(&args).await;
    debug!("args: {:?}", args);

    notifier.report_status("Connecting to T2V module".into());
    debug!("enumerating devices");
    let devices = match T2VModule::find_connected_custom(args.vendor_id, args.product_id).await {
        Ok(devices) => devices,
        Err(error) => {
            error!("Failed to enumerate devices: {error}");
            exit(-1);
        }
    };

    notifier.ready();
    notifier.report_status("Connected".into());

    match unit::unit(devices, &socket, notifier.as_ref()).await {
        Ok(()) => info!("Device closed cleanly"),
        Err(e) => error!("Error while running unit: {e}"),
    }

    notifier.report_status("Reconnecting".into());
}

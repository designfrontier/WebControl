//! Command line, startup and shutdown of the `webcontrol-rs` binary.

use std::future::Future;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;
use wc_config::{Paths, Settings};
use wc_firmware::image::Image;
use wc_machine::gpio::{Gpio, Plan};
use wc_machine::{Event, Machine, MachineHandle, PortOpener, spawn};
use wc_optical::camera::SledPosition;
use wc_sim::{Build, Sim};

use crate::camera::Cameras;
use crate::firmware::{SimFlasher, simulated_target};
use crate::host::SystemHost;
use crate::serial::{SerialOpener, SimOpener};
use crate::{AppState, PingConfig, router};

pub const HELP: &str = "\
Usage: webcontrol-rs [--port N] [--home DIR] [--simulate]

  --port N     listen on port N (default: webPort from webcontrol.json, else 5000; 0 picks one)
  --home DIR   use DIR/.WebControl instead of $HOME/.WebControl
  --simulate   talk to a simulated holey 51.27 controller on PCB v1.1 instead of the serial port
  --version    print the version, git commit and contract version
  --help       print this help";

/// How long shutting down waits for the settings writer.
const SAVE_ON_EXIT: std::time::Duration = std::time::Duration::from_secs(5);

/// The port name `--simulate` connects to.
pub const SIMULATED_PORT: &str = "simulator";
/// The owner's board, which the upgrade wizard can be rehearsed on.
pub const SIMULATED_BOARD: &str = "PCB v1.1 Detected";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub home: PathBuf,
    pub port: Option<u16>,
    pub simulate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Serve(Options),
    Version,
    Help,
}

/// A literal in the binary, so `rust/scripts/release.sh` can check a build it cannot run.
pub const VERSION_PREFIX: &str = concat!("webcontrol-rs ", env!("WC_VERSION"), " (", env!("WC_GIT_SHA"));

pub fn version_text() -> String {
    format!("{VERSION_PREFIX}, contract {})", wc_protocol::CONTRACT_VERSION)
}

/// `args` without the program name; `home` is `$HOME`.
pub fn parse(args: &[String], home: Option<PathBuf>) -> Result<Command, String> {
    let mut port = None;
    let mut home = home;
    let mut simulate = false;
    let mut rest = args.iter().flat_map(|arg| match arg.split_once('=') {
        Some((flag, value)) if flag.starts_with("--") => vec![flag.to_string(), value.to_string()],
        _ => vec![arg.clone()],
    });
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--version" | "-V" => return Ok(Command::Version),
            "--help" | "-h" => return Ok(Command::Help),
            "--port" => {
                let value = rest.next().ok_or("--port needs a number")?;
                port = Some(value.parse().map_err(|_| format!("--port {value:?} is not a port number"))?);
            }
            "--home" => home = Some(rest.next().ok_or("--home needs a directory")?.into()),
            "--simulate" => simulate = true,
            other => return Err(format!("unknown argument {other:?}\n\n{HELP}")),
        }
    }
    let home = home.ok_or("HOME is not set; pass --home")?;
    Ok(Command::Serve(Options { home, port, simulate }))
}

/// The address other machines reach this one at; connecting a UDP socket sends nothing.
fn local_ip() -> IpAddr {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| socket.connect("192.0.2.1:9").map(|_| socket))
        .and_then(|socket| socket.local_addr())
        .map(|address| address.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

pub async fn serve(options: Options, shutdown: impl Future<Output = ()> + Send + 'static) -> std::io::Result<()> {
    let paths = Paths::new(&options.home);
    let (settings, saving) = Settings::open(&paths).map(|settings| (settings, true)).unwrap_or_else(|error| {
        tracing::error!(%error, "could not prepare ~/.WebControl; settings will not be saved");
        (Settings::load_derived(&paths), false)
    });
    tracing::info!(source = ?settings.source(), home = %options.home.display(), "settings loaded");
    let port = options.port.unwrap_or_else(|| settings.web_port());
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await.map_err(|error| {
        match error.kind() {
            std::io::ErrorKind::AddrInUse => std::io::Error::new(
                error.kind(),
                format!("port {port} is in use, perhaps by an older WebControl (see `wc-switch status`)"),
            ),
            _ => error,
        }
    })?;
    let bound = listener.local_addr()?;
    let exit = Arc::new(Notify::new());
    let host = SystemHost::new(&paths.dir(), exit.clone());
    let host = if saving { host.saving_to(paths.clone()) } else { host };
    let machine = Machine::new(&format!("{}:{}", local_ip(), bound.port()), settings).with_library(paths.gcode_dir());
    let (machine, opener, host, cameras): (Machine, Box<dyn PortOpener>, SystemHost, Cameras) = match options.simulate {
        true => {
            let opener = SimOpener::new(Build::holey_51_27().on_board(SIMULATED_BOARD));
            let host = host.with_flasher(Box::new(SimFlasher { sim: opener.sim() }));
            let machine = machine.on_port(SIMULATED_PORT).with_firmware(Image::embedded().or(Some(simulated_target())));
            let cameras = Cameras::new(Some(sled_position(opener.sim())));
            (machine, Box::new(opener), host, cameras)
        }
        false => (machine, Box::new(SerialOpener), host, Cameras::default()),
    };
    let host = Arc::new(host);
    let (machine, task) = spawn(machine, opener, host.clone());
    host.attach_gpio(Some(start_gpio(&machine)));
    println!("webcontrol-rs listening on {bound}");
    std::io::stdout().flush()?;
    let streaming = cameras.stream_to(machine.clone());
    let state = AppState::new(machine, PingConfig::default(), paths).with_cameras(cameras);
    let closing = state.clone();
    let stopped = async move {
        tokio::select! {
            _ = shutdown => {}
            _ = exit.notified() => tracing::info!("shut down from the UI"),
        }
        closing.close_sessions();
    };
    axum::serve(listener, router(state)).with_graceful_shutdown(stopped).await?;
    host.attach_gpio(None);
    drop(streaming);
    // The machine task ends with the last handle, once its final settings save is on disk.
    let _ = tokio::time::timeout(SAVE_ON_EXIT, task).await;
    Ok(())
}

/// The simulated sled's X and Y in mm, for the synthetic camera.
pub fn sled_position(sim: Arc<Mutex<Sim>>) -> SledPosition {
    Arc::new(move || {
        let [x, y, _] = sim.lock().expect("the simulator does not panic").position();
        (x, y)
    })
}

/// The buttons and LEDs `GPIO Settings` names; a press reaches the machine as the UI's action would.
fn start_gpio(machine: &MachineHandle) -> Gpio {
    let plan = Plan::from_settings(&machine.snapshot().settings);
    let presses = machine.clone();
    let started = std::time::Instant::now();
    let press = Arc::new(move |button| presses.send(Event::Button(button)));
    Gpio::start(&plan, crate::gpio::open(), press, Arc::new(move || started.elapsed()))
}

async fn terminated() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    tracing::info!("shutting down");
}

/// The binary's whole life; returns the exit code.
pub fn run(args: &[String], home: Option<PathBuf>) -> i32 {
    match parse(args, home) {
        Ok(Command::Version) => println!("{}", version_text()),
        Ok(Command::Help) => println!("{HELP}"),
        Ok(Command::Serve(options)) => {
            let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
            tracing_subscriber::fmt().with_env_filter(filter).init();
            let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build();
            if let Err(error) = runtime.and_then(|rt| rt.block_on(serve(options, terminated()))) {
                eprintln!("webcontrol-rs: {error}");
                return 1;
            }
        }
        Err(message) => {
            eprintln!("webcontrol-rs: {message}");
            return 2;
        }
    }
    0
}

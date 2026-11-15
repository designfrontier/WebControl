//! Command line, startup and shutdown of the `webcontrol-rs` binary.

use std::future::Future;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Notify;
use wc_config::{Paths, Settings};
use wc_machine::{Machine, PortOpener, spawn};
use wc_sim::Build;

use crate::host::SystemHost;
use crate::serial::{SerialOpener, SimOpener};
use crate::{AppState, PingConfig, router};

pub const HELP: &str = "\
Usage: webcontrol-rs [--port N] [--home DIR] [--simulate]

  --port N     listen on port N (default: webPort from webcontrol.json, else 5000; 0 picks one)
  --home DIR   use DIR/.WebControl instead of $HOME/.WebControl
  --simulate   talk to a simulated holey 51.27 controller instead of the serial port
  --version    print the version, git commit and contract version
  --help       print this help";

/// The port name `--simulate` connects to.
pub const SIMULATED_PORT: &str = "simulator";

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

pub fn version_text() -> String {
    format!(
        "webcontrol-rs {} ({}, contract {})",
        env!("CARGO_PKG_VERSION"),
        env!("WC_GIT_SHA"),
        wc_protocol::CONTRACT_VERSION
    )
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
    [paths.gcode_dir(), paths.imports_dir(), paths.boards_dir()].iter().try_for_each(std::fs::create_dir_all)?;
    let settings = Settings::load(&paths);
    tracing::info!(source = ?settings.source(), home = %options.home.display(), "settings loaded");
    let port = options.port.unwrap_or_else(|| settings.web_port());
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;
    let bound = listener.local_addr()?;
    let exit = Arc::new(Notify::new());
    let host = Arc::new(SystemHost::new(&paths.dir(), exit.clone()));
    let machine = Machine::new(&format!("{}:{}", local_ip(), bound.port()), settings);
    let (machine, opener): (Machine, Box<dyn PortOpener>) = match options.simulate {
        true => (machine.on_port(SIMULATED_PORT), Box::new(SimOpener::new(Build::holey_51_27()))),
        false => (machine, Box::new(SerialOpener)),
    };
    let (machine, _task) = spawn(machine, opener, host);
    println!("webcontrol-rs listening on {bound}");
    std::io::stdout().flush()?;
    let state = AppState::new(machine, PingConfig::default(), paths);
    let closing = state.clone();
    let stopped = async move {
        tokio::select! {
            _ = shutdown => {}
            _ = exit.notified() => tracing::info!("shut down from the UI"),
        }
        closing.close_sessions();
    };
    axum::serve(listener, router(state)).with_graceful_shutdown(stopped).await
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

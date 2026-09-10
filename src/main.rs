use std::io::Write;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use recent_spaces::api::{self, Client};
use recent_spaces::claim::FileClaim;
use recent_spaces::config::{self, Environment};
use recent_spaces::promote::{self, Dwell};
use recent_spaces::retire;
use recent_spaces::version;

fn main() {
    match version::requested(&std::env::args_os().skip(1).collect::<Vec<_>>()) {
        version::Request::Watch => {
            if catch_unwind_of(run).is_err() {
                note("the watcher stopped on a fault it could not handle");
            }
        }
        version::Request::Report => report_the_build(),
        version::Request::Refuse(argument) => refuse(&argument),
    }
}

fn report_the_build() {
    let env = Environment::from_process();
    let manifest = version::read_manifest(version::root_of(&env).as_deref());
    let origin = version::origin_of(std::env::current_exe().ok());
    print!(
        "{}",
        version::report(env!("CARGO_BIN_NAME"), &manifest, origin)
    );
}

fn refuse(argument: &str) -> ! {
    note(&format!(
        "unknown argument `{}`; run it with no arguments to watch, or `{}` to report the build",
        argument,
        version::FLAG
    ));
    std::process::exit(2);
}

fn catch_unwind_of(body: fn()) -> Result<(), ()> {
    std::panic::catch_unwind(body).map_err(|_| ())
}

fn run() {
    let env = Environment::from_process();
    let settings = config::resolve_settings(
        &env,
        &config::read_sources(&env, &config::plugin_root(&env)),
    );
    for complaint in &settings.complaints {
        note(complaint);
    }

    let socket = api::socket_path(&env);
    let client = Client::new(socket.clone());
    let claim = FileClaim::new(&config::state_dir(&env), &socket);
    let mut dwell = Dwell::new();
    let started = Instant::now();

    retire::watch(
        &claim,
        &token(),
        |now| promote::tick(&client, &settings, &mut dwell, now).map_err(|e| e.to_string()),
        || started.elapsed().as_secs_f64(),
        |seconds| std::thread::sleep(Duration::from_secs_f64(seconds)),
    );
}

fn token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{:x}", std::process::id(), nanos)
}

fn note(message: &str) {
    let _ = writeln!(std::io::stderr(), "recent-spaces: {}", message);
}

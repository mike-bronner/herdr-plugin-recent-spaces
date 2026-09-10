use std::io::Write;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use recent_spaces::api::{self, Client};
use recent_spaces::claim::FileClaim;
use recent_spaces::config::{self, Environment};
use recent_spaces::promote::{self, Dwell};
use recent_spaces::retire;
use recent_spaces::version;

fn main() {
    if asked_for_the_version() {
        let env = Environment::from_process();
        let manifest = version::read_manifest(version::root_of(&env).as_deref());
        print!("{}", version::report(env!("CARGO_BIN_NAME"), &manifest));
        return;
    }
    if catch_unwind_of(run).is_err() {
        note("the watcher stopped on a fault it could not handle");
    }
}

fn asked_for_the_version() -> bool {
    std::env::args_os()
        .nth(1)
        .map(|first| first == version::FLAG)
        .unwrap_or(false)
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

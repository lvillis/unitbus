#[cfg(feature = "observe")]
use unitbus::{ObserveOptions, UnitBus};

#[cfg(not(feature = "observe"))]
fn main() {
    eprintln!("This example requires `--features observe`.");
}

#[cfg(all(feature = "observe", feature = "rt-async-io"))]
fn main() {
    if let Err(e) = smol::block_on(run()) {
        eprintln!("{e:?}");
        std::process::exit(1);
    }
}

#[cfg(all(feature = "observe", feature = "rt-tokio"))]
fn main() {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("init tokio runtime failed: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = rt.block_on(run()) {
        eprintln!("{e:?}");
        std::process::exit(1);
    }
}

#[cfg(feature = "observe")]
async fn run() -> Result<(), unitbus::Error> {
    let unit = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "nginx".to_string());

    let bus = UnitBus::connect_system().await?;
    let mut watcher = bus
        .observe()
        .watch_unit_failure(&unit, ObserveOptions::default())
        .await?;

    eprintln!("Watching for failures: {unit}");
    let ev = watcher.next().await?;
    match ev {
        Some(ev) => {
            eprintln!("unit failed: {:?}", ev.status);
            if let Some(diag) = ev.diagnosis {
                eprintln!("logs={}", diag.logs.len());
            }
            if let Some(err) = ev.diagnosis_error {
                eprintln!("diagnosis error: {err:?}");
            }
        }
        None => {
            eprintln!("stream ended");
        }
    }

    Ok(())
}

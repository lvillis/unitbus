use std::time::Duration;

use unitbus::{JobOutcome, UnitBus, UnitStartMode};

#[cfg(feature = "rt-async-io")]
fn main() {
    if let Err(e) = smol::block_on(run()) {
        eprintln!("{e:?}");
        std::process::exit(1);
    }
}

#[cfg(feature = "rt-tokio")]
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

async fn run() -> Result<(), unitbus::Error> {
    let bus = UnitBus::connect_system().await?;

    let job = bus.units().restart("nginx", UnitStartMode::Replace).await?;

    let outcome = job.wait(Duration::from_secs(30)).await?;
    match outcome {
        JobOutcome::Success { .. } => {
            println!("restart success");
        }
        JobOutcome::Failed { .. } | JobOutcome::Canceled { .. } => {
            let diag = bus
                .journal()
                .diagnose_unit_failure("nginx", Default::default())
                .await?;
            println!("status={:?}", diag.status);
            for e in diag.logs {
                println!("{:?} {:?}", e.timestamp, e.message);
            }
        }
        _ => {
            let diag = bus
                .journal()
                .diagnose_unit_failure("nginx", Default::default())
                .await?;
            println!("status={:?}", diag.status);
            for e in diag.logs {
                println!("{:?} {:?}", e.timestamp, e.message);
            }
        }
    }

    Ok(())
}

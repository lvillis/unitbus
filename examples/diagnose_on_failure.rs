use std::time::Duration;

use unitbus::{JobOutcome, UnitBus, UnitStartMode};

fn main() {
    if let Err(e) = smol::block_on(run()) {
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

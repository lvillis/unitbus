use std::time::Duration;

use unitbus::{UnitBus, UnitStartMode};

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
    println!("{outcome:?}");
    Ok(())
}

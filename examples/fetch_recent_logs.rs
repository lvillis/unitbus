use std::time::Duration;

use unitbus::{JournalFilter, UnitBus};

fn main() {
    if let Err(e) = smol::block_on(run()) {
        eprintln!("{e:?}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), unitbus::Error> {
    let bus = UnitBus::connect_system().await?;

    let mut filter = JournalFilter::default();
    filter.unit = Some("nginx".to_string());
    filter.limit = 50;
    filter.timeout = Some(Duration::from_secs(5));

    let res = bus.journal().query(filter).await?;
    for e in res.entries {
        println!("{:?} {:?}", e.timestamp, e.message);
    }

    Ok(())
}

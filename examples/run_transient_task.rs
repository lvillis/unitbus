#[cfg(feature = "tasks")]
use std::collections::BTreeMap;
#[cfg(feature = "tasks")]
use std::time::Duration;

#[cfg(feature = "tasks")]
use unitbus::{TaskSpec, UnitBus};

#[cfg(not(feature = "tasks"))]
fn main() {
    eprintln!("This example requires `--features tasks`.");
}

#[cfg(feature = "tasks")]
fn main() {
    if let Err(e) = smol::block_on(run()) {
        eprintln!("{e:?}");
        std::process::exit(1);
    }
}

#[cfg(feature = "tasks")]
async fn run() -> Result<(), unitbus::Error> {
    let bus = UnitBus::connect_system().await?;

    let mut spec = TaskSpec::default();
    spec.argv = vec!["/bin/echo".to_string(), "hello".to_string()];
    spec.env = BTreeMap::new();
    spec.timeout = Duration::from_secs(10);
    spec.name_hint = Some("demo".to_string());

    let task = bus.tasks().run(spec).await?;
    let res = task.wait(Duration::from_secs(30)).await?;
    println!("{res:?}");
    Ok(())
}

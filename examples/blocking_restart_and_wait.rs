#[cfg(feature = "blocking")]
use std::time::Duration;

#[cfg(feature = "blocking")]
use unitbus::{BlockingUnitBus, UnitStartMode};

#[cfg(not(feature = "blocking"))]
fn main() {
    eprintln!("This example requires `--features blocking`.");
}

#[cfg(feature = "blocking")]
fn main() {
    let unit = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "nginx".to_string());

    let bus = match BlockingUnitBus::connect_system() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e:?}");
            std::process::exit(1);
        }
    };

    let job = match bus.units().restart(&unit, UnitStartMode::Replace) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("{e:?}");
            std::process::exit(1);
        }
    };

    let outcome = match job.wait(Duration::from_secs(30)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e:?}");
            std::process::exit(1);
        }
    };

    println!("{outcome:?}");
}

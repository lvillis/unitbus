use unitbus::UnitBus;

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

    let info = bus.manager().info().await?;
    eprintln!(
        "system_state={:?} version={:?}",
        info.system_state, info.version
    );

    let units = bus.manager().list_units_filtered(&["active"]).await?;
    for u in units.into_iter().take(20) {
        let unit_props = bus
            .units()
            .get_unit_properties_by_path(&u.unit_path)
            .await?;
        let id = unit_props.get_opt_str("Id").unwrap_or("<unknown>");

        let main_pid = match bus
            .units()
            .get_service_properties_by_path(&u.unit_path)
            .await?
        {
            Some(service_props) => service_props.get_u32("MainPID"),
            None => None,
        };

        println!("{id} pid={main_pid:?}");
    }

    Ok(())
}

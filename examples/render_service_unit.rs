use std::collections::BTreeMap;

fn main() -> Result<(), unitbus::Error> {
    let mut env = BTreeMap::new();
    env.insert("RUST_LOG".to_string(), "info".to_string());

    let mut spec = unitbus::ServiceUnitSpec::default();
    spec.unit = "demo".to_string();
    spec.description = Some("Demo service managed by unitbus".to_string());
    spec.after = vec!["network-online.target".to_string()];
    spec.wants = vec!["network-online.target".to_string()];
    spec.service_type = Some(unitbus::ServiceType::Simple);
    spec.exec_start = vec!["/usr/bin/demo".to_string(), "--serve".to_string()];
    spec.working_directory = Some("/srv/demo".to_string());
    spec.user = Some("demo".to_string());
    spec.group = Some("demo".to_string());
    spec.environment = env;
    spec.restart = Some("always".to_string());
    spec.restart_sec = Some(3);
    spec.timeout_start_sec = Some(10);
    spec.wanted_by = vec!["multi-user.target".to_string()];

    let rendered = spec.render()?;
    print!("{rendered}");
    Ok(())
}

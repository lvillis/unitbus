/// Probe capabilities conservatively (never guess true).
pub(crate) async fn probe(bus: &crate::UnitBus) -> Capabilities {
    let can_read_units = bus.units().get_status("dbus.service").await.is_ok();

    let can_control_units = probe_control_units(bus).await;

    let can_read_journal = {
        #[cfg(any(feature = "journal-cli", feature = "journal-sdjournal"))]
        {
            let filter = crate::types::journal::JournalFilter {
                limit: 1,
                ..Default::default()
            };
            bus.journal().query(filter).await.is_ok()
        }

        #[cfg(not(any(feature = "journal-cli", feature = "journal-sdjournal")))]
        {
            false
        }
    };

    let can_write_dropins = {
        #[cfg(feature = "config")]
        {
            probe_write_dropins(&bus.inner.opts.systemd_system_dir)
        }

        #[cfg(not(feature = "config"))]
        {
            false
        }
    };

    Capabilities {
        can_read_units,
        can_control_units,
        can_read_journal,
        can_write_dropins,
    }
}

async fn probe_control_units(bus: &crate::UnitBus) -> bool {
    let proxy = match bus.inner.bus.manager_proxy().await {
        Ok(p) => p,
        Err(_) => return false,
    };

    let two_arg: std::result::Result<String, zbus::Error> = proxy
        .call("CanStartUnit", &("dbus.service", "replace"))
        .await;

    let res = match two_arg {
        Ok(s) => Ok(s),
        Err(zbus::Error::MethodError(name, _, _)) if name.contains("InvalidArgs") => {
            proxy.call("CanStartUnit", &("dbus.service")).await
        }
        Err(e) => Err(e),
    };

    match res {
        Ok(answer) => answer == "yes",
        Err(zbus::Error::MethodError(name, _, _))
            if name.contains("UnknownMethod")
                || name.contains("UnknownMember")
                || name.contains("UnknownInterface") =>
        {
            false
        }
        Err(_) => false,
    }
}

#[cfg(feature = "config")]
fn probe_write_dropins(_systemd_system_dir: &str) -> bool {
    #[cfg(unix)]
    {
        probe_write_dropins_unix(_systemd_system_dir)
    }

    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(all(unix, feature = "config"))]
fn probe_write_dropins_unix(path: &str) -> bool {
    use std::os::unix::fs::MetadataExt;

    if is_mount_read_only(path) {
        return false;
    }

    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if !meta.is_dir() {
        return false;
    }

    let creds = match ProcCreds::read() {
        Some(c) => c,
        None => return false,
    };

    if creds.euid == 0 {
        return true;
    }

    let mode = meta.mode();
    let uid = meta.uid();
    let gid = meta.gid();

    let (w, x) = if creds.euid == uid {
        (0o200, 0o100)
    } else if creds.groups.contains(&gid) {
        (0o020, 0o010)
    } else {
        (0o002, 0o001)
    };

    (mode & w != 0) && (mode & x != 0)
}

#[cfg(all(unix, feature = "config"))]
fn is_mount_read_only(path: &str) -> bool {
    let mounts = match std::fs::read_to_string("/proc/mounts") {
        Ok(s) => s,
        Err(_) => return false,
    };

    let mut best_mount_len = 0usize;
    let mut best_ro = false;

    for line in mounts.lines() {
        let mut it = line.split_whitespace();
        let _dev = it.next();
        let mountpoint = match it.next() {
            Some(v) => v,
            None => continue,
        };
        let _fstype = it.next();
        let opts = match it.next() {
            Some(v) => v,
            None => continue,
        };

        let mountpoint = unescape_mount_field(mountpoint);
        if !is_under_mount(path, &mountpoint) {
            continue;
        }
        if mountpoint.len() >= best_mount_len {
            best_mount_len = mountpoint.len();
            best_ro = opts.split(',').any(|o| o == "ro");
        }
    }

    best_ro
}

#[cfg(all(unix, feature = "config"))]
fn is_under_mount(path: &str, mountpoint: &str) -> bool {
    if mountpoint == "/" {
        return path.starts_with('/');
    }
    if path == mountpoint {
        return true;
    }
    if !path.starts_with(mountpoint) {
        return false;
    }
    path.as_bytes()
        .get(mountpoint.len())
        .copied()
        .is_some_and(|b| b == b'/')
}

#[cfg(all(unix, feature = "config"))]
fn unescape_mount_field(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }

        let (Some(a), Some(b), Some(c)) = (chars.next(), chars.next(), chars.next()) else {
            out.push('\\');
            break;
        };
        if a.is_ascii_digit() && b.is_ascii_digit() && c.is_ascii_digit() {
            let oct = [a, b, c]
                .iter()
                .filter_map(|ch| ch.to_digit(8))
                .fold(0u32, |acc, v| acc * 8 + v);
            if let Some(ch) = char::from_u32(oct) {
                out.push(ch);
            }
        } else {
            out.push('\\');
            out.push(a);
            out.push(b);
            out.push(c);
        }
    }
    out
}

#[cfg(all(unix, feature = "config"))]
struct ProcCreds {
    euid: u32,
    groups: Vec<u32>,
}

#[cfg(all(unix, feature = "config"))]
impl ProcCreds {
    fn read() -> Option<Self> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let mut euid = None;
        let mut groups = Vec::<u32>::new();

        for line in status.lines() {
            if line.starts_with("Uid:") {
                let mut it = line.split_whitespace();
                let _label = it.next()?;
                let _ruid = it.next()?;
                let euid_s = it.next()?;
                euid = euid_s.parse::<u32>().ok();
            } else if line.starts_with("Groups:") {
                let mut it = line.split_whitespace();
                let _label = it.next()?;
                for g in it {
                    if let Ok(gid) = g.parse::<u32>() {
                        groups.push(gid);
                    }
                }
            }
        }

        Some(Self {
            euid: euid?,
            groups,
        })
    }
}

/// Runtime capabilities derived from conservative probing.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Capabilities {
    /// Whether status queries are likely to work (D-Bus connectivity + read access).
    pub can_read_units: bool,
    /// Whether unit control operations are likely to be authorized.
    pub can_control_units: bool,
    /// Whether journald queries are likely to work (backend available + permission).
    pub can_read_journal: bool,
    /// Whether drop-in writes under `/etc/systemd/system` are likely to succeed.
    pub can_write_dropins: bool,
}

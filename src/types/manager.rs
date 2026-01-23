use crate::{ActiveState, LoadState};

/// A single row returned by `org.freedesktop.systemd1.Manager.ListUnits*`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct UnitListEntry {
    pub name: String,
    pub description: Option<String>,
    pub load_state: LoadState,
    pub active_state: ActiveState,
    pub sub_state: Option<String>,
    pub followed: Option<String>,
    pub unit_path: String,
    pub job_id: Option<u32>,
    pub job_type: Option<String>,
    pub job_path: Option<String>,
}

impl UnitListEntry {
    pub(crate) fn from_dbus(item: crate::bus::ListUnitItem) -> Self {
        let (
            name,
            description,
            load_state,
            active_state,
            sub_state,
            followed,
            unit_path,
            job_id,
            job_type,
            job_path,
        ) = item;

        let description = if description.is_empty() {
            None
        } else {
            Some(description)
        };
        let sub_state = if sub_state.is_empty() {
            None
        } else {
            Some(sub_state)
        };
        let followed = if followed.is_empty() {
            None
        } else {
            Some(followed)
        };

        let has_job = job_id != 0 && job_path.as_str() != "/";
        let (job_id, job_type, job_path) = if has_job {
            (
                Some(job_id),
                if job_type.is_empty() {
                    None
                } else {
                    Some(job_type)
                },
                Some(job_path.to_string()),
            )
        } else {
            (None, None, None)
        };

        Self {
            name,
            description,
            load_state: LoadState::parse(&load_state),
            active_state: ActiveState::parse(&active_state),
            sub_state,
            followed,
            unit_path: unit_path.to_string(),
            job_id,
            job_type,
            job_path,
        }
    }
}

/// A small snapshot of `org.freedesktop.systemd1.Manager` global information.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct ManagerInfo {
    pub system_state: Option<String>,
    pub version: Option<String>,
    pub virtualization: Option<String>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]
    #![allow(clippy::unwrap_used)]

    use super::*;
    use zbus::zvariant::OwnedObjectPath;

    fn path(s: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(s).expect("valid object path")
    }

    #[test]
    fn list_units_decodes_job_fields_as_none_when_not_present() {
        let item = (
            "nginx.service".to_string(),
            "nginx".to_string(),
            "loaded".to_string(),
            "active".to_string(),
            "running".to_string(),
            "".to_string(),
            path("/org/freedesktop/systemd1/unit/nginx_2eservice"),
            0u32,
            "".to_string(),
            path("/"),
        );

        let e = UnitListEntry::from_dbus(item);
        assert_eq!(e.name, "nginx.service");
        assert_eq!(e.load_state, LoadState::Loaded);
        assert_eq!(e.active_state, ActiveState::Active);
        assert_eq!(e.sub_state.as_deref(), Some("running"));
        assert_eq!(e.job_id, None);
        assert_eq!(e.job_type, None);
        assert_eq!(e.job_path, None);
    }

    #[test]
    fn list_units_decodes_job_fields_when_present_and_normalizes_empty_strings() {
        let item = (
            "x.service".to_string(),
            "".to_string(),
            "loaded".to_string(),
            "inactive".to_string(),
            "".to_string(),
            "".to_string(),
            path("/org/freedesktop/systemd1/unit/x_2eservice"),
            123u32,
            "start".to_string(),
            path("/org/freedesktop/systemd1/job/123"),
        );

        let e = UnitListEntry::from_dbus(item);
        assert_eq!(e.description, None);
        assert_eq!(e.sub_state, None);
        assert_eq!(e.followed, None);
        assert_eq!(e.job_id, Some(123));
        assert_eq!(e.job_type.as_deref(), Some("start"));
        assert_eq!(
            e.job_path.as_deref(),
            Some("/org/freedesktop/systemd1/job/123")
        );
    }
}

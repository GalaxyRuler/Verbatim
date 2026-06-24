use serde::Serialize;
use specta::Type;

const LINUX_HELPERS: &[&str] = &["wl-copy", "wtype", "kwtype", "dotool", "ydotool", "xdotool"];

#[derive(Clone, Debug, Serialize, Type)]
pub struct LinuxHelperStatus {
    pub name: String,
    pub available: bool,
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct LinuxEnvironmentStatus {
    pub is_linux: bool,
    pub session_type: String,
    pub desktop: String,
    pub is_wayland: bool,
    pub is_x11: bool,
    pub helpers: Vec<LinuxHelperStatus>,
    pub clipboard_helper: Option<String>,
    pub key_combo_helper: Option<String>,
    pub direct_input_helper: Option<String>,
    pub at_spi_available: bool,
    pub tray_status: String,
    pub warnings: Vec<String>,
}

pub fn linux_environment_status() -> LinuxEnvironmentStatus {
    #[cfg(target_os = "linux")]
    {
        let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into());
        let is_wayland = crate::utils::is_wayland();
        let is_x11 = !is_wayland
            && (session_type.eq_ignore_ascii_case("x11") || std::env::var("DISPLAY").is_ok());
        let is_kde = crate::utils::is_kde_plasma();
        let at_spi_available = detect_at_spi_available();
        let tray_status = infer_tray_status(&desktop);
        let available_helpers = LINUX_HELPERS
            .iter()
            .copied()
            .filter(|helper| command_exists(helper))
            .collect::<Vec<_>>();

        linux_environment_status_from_parts(
            true,
            session_type,
            desktop,
            is_wayland,
            is_x11,
            is_kde,
            at_spi_available,
            tray_status,
            &available_helpers,
        )
    }

    #[cfg(not(target_os = "linux"))]
    {
        linux_environment_status_from_parts(
            false,
            "unsupported".into(),
            "unsupported".into(),
            false,
            false,
            false,
            false,
            "unsupported".into(),
            &[],
        )
    }
}

fn linux_environment_status_from_parts(
    is_linux: bool,
    session_type: String,
    desktop: String,
    is_wayland: bool,
    is_x11: bool,
    is_kde: bool,
    at_spi_available: bool,
    tray_status: String,
    available_helpers: &[&str],
) -> LinuxEnvironmentStatus {
    let helper_available = |name: &str| available_helpers.contains(&name);
    let clipboard_helper = is_wayland
        .then(|| helper_available("wl-copy").then_some("wl-copy".to_string()))
        .flatten();
    let direct_input_helper =
        select_direct_input_helper(is_wayland, is_x11, is_kde, available_helpers);
    let key_combo_helper = select_key_combo_helper(is_wayland, is_x11, is_kde, available_helpers);

    let helpers = LINUX_HELPERS
        .iter()
        .map(|helper| LinuxHelperStatus {
            name: (*helper).to_string(),
            available: helper_available(helper),
            roles: helper_roles(*helper, is_wayland, is_x11, is_kde),
        })
        .collect::<Vec<_>>();

    let mut warnings = Vec::new();
    if is_linux && !is_wayland && !is_x11 {
        warnings.push("unknown_session_type".to_string());
    }
    if is_linux && is_wayland && clipboard_helper.is_none() {
        warnings.push("missing_wayland_clipboard_helper".to_string());
    }
    if is_linux && direct_input_helper.is_none() {
        warnings.push("missing_direct_input_helper".to_string());
    }
    if is_linux && key_combo_helper.is_none() {
        warnings.push("missing_key_combo_helper".to_string());
    }
    if is_linux && !at_spi_available {
        warnings.push("missing_at_spi".to_string());
    }
    if is_linux && tray_status != "likely_available" {
        warnings.push("tray_uncertain".to_string());
    }

    LinuxEnvironmentStatus {
        is_linux,
        session_type,
        desktop,
        is_wayland,
        is_x11,
        helpers,
        clipboard_helper,
        key_combo_helper,
        direct_input_helper,
        at_spi_available,
        tray_status,
        warnings,
    }
}

fn select_direct_input_helper(
    is_wayland: bool,
    is_x11: bool,
    is_kde: bool,
    available_helpers: &[&str],
) -> Option<String> {
    let helper_available = |name: &str| available_helpers.contains(&name);

    if is_wayland {
        if is_kde && helper_available("kwtype") {
            return Some("kwtype".to_string());
        }
        if !is_kde && helper_available("wtype") {
            return Some("wtype".to_string());
        }
        if helper_available("dotool") {
            return Some("dotool".to_string());
        }
        if helper_available("ydotool") {
            return Some("ydotool".to_string());
        }
    }

    if is_x11 {
        if helper_available("xdotool") {
            return Some("xdotool".to_string());
        }
        if helper_available("ydotool") {
            return Some("ydotool".to_string());
        }
    }

    None
}

fn select_key_combo_helper(
    is_wayland: bool,
    is_x11: bool,
    is_kde: bool,
    available_helpers: &[&str],
) -> Option<String> {
    let helper_available = |name: &str| available_helpers.contains(&name);

    if is_wayland {
        if !is_kde && helper_available("wtype") {
            return Some("wtype".to_string());
        }
        if helper_available("dotool") {
            return Some("dotool".to_string());
        }
        if helper_available("ydotool") {
            return Some("ydotool".to_string());
        }
    }

    if is_x11 {
        if helper_available("xdotool") {
            return Some("xdotool".to_string());
        }
        if helper_available("ydotool") {
            return Some("ydotool".to_string());
        }
    }

    None
}

fn helper_roles(helper: &str, is_wayland: bool, is_x11: bool, is_kde: bool) -> Vec<String> {
    let mut roles = Vec::new();

    if helper == "wl-copy" && is_wayland {
        roles.push("clipboard".to_string());
    }
    if is_wayland {
        if helper == "kwtype" && is_kde {
            roles.push("direct_input".to_string());
        }
        if helper == "wtype" && !is_kde {
            roles.push("direct_input".to_string());
            roles.push("key_combo".to_string());
        }
        if matches!(helper, "dotool" | "ydotool") {
            roles.push("direct_input".to_string());
            roles.push("key_combo".to_string());
        }
    }
    if is_x11 && matches!(helper, "xdotool" | "ydotool") {
        roles.push("direct_input".to_string());
        roles.push("key_combo".to_string());
    }

    roles
}

#[cfg(target_os = "linux")]
fn detect_at_spi_available() -> bool {
    std::env::var("NO_AT_BRIDGE")
        .map(|value| value != "1")
        .unwrap_or(true)
        && std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok()
}

#[cfg(target_os = "linux")]
fn infer_tray_status(desktop: &str) -> String {
    let desktop = desktop.to_uppercase();
    if ["KDE", "XFCE", "X-CINNAMON", "CINNAMON", "MATE", "LXQT"]
        .iter()
        .any(|candidate| desktop.contains(candidate))
    {
        "likely_available".to_string()
    } else if desktop.contains("GNOME") {
        "may_require_extension".to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(target_os = "linux")]
fn command_exists(command: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {} >/dev/null 2>&1", command))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_kwtype_for_kde_wayland_direct_input() {
        let status = linux_environment_status_from_parts(
            true,
            "wayland".into(),
            "KDE".into(),
            true,
            false,
            true,
            true,
            "likely_available".into(),
            &["wtype", "kwtype", "ydotool", "wl-copy"],
        );

        assert_eq!(status.direct_input_helper.as_deref(), Some("kwtype"));
        assert_eq!(status.key_combo_helper.as_deref(), Some("ydotool"));
        assert_eq!(status.clipboard_helper.as_deref(), Some("wl-copy"));
        assert!(status.warnings.is_empty());
    }

    #[test]
    fn reports_missing_wayland_helpers() {
        let status = linux_environment_status_from_parts(
            true,
            "wayland".into(),
            "GNOME".into(),
            true,
            false,
            false,
            false,
            "may_require_extension".into(),
            &[],
        );

        assert_eq!(
            status.warnings,
            vec![
                "missing_wayland_clipboard_helper",
                "missing_direct_input_helper",
                "missing_key_combo_helper",
                "missing_at_spi",
                "tray_uncertain"
            ]
        );
    }

    #[test]
    fn prefers_xdotool_for_x11() {
        let status = linux_environment_status_from_parts(
            true,
            "x11".into(),
            "GNOME".into(),
            false,
            true,
            false,
            true,
            "likely_available".into(),
            &["ydotool", "xdotool"],
        );

        assert_eq!(status.direct_input_helper.as_deref(), Some("xdotool"));
        assert_eq!(status.key_combo_helper.as_deref(), Some("xdotool"));
        assert!(status.clipboard_helper.is_none());
        assert!(status.warnings.is_empty());
    }
}

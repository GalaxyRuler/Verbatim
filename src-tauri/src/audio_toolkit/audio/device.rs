use cpal::traits::{DeviceTrait, HostTrait};

pub struct CpalDeviceInfo {
    pub index: String,
    pub name: String,
    pub stable_id: Option<String>,
    pub is_default: bool,
    pub device: cpal::Device,
}

fn has_legacy_driver_suffix(short_name: &str, legacy_name: &str) -> bool {
    legacy_name
        .strip_prefix(short_name)
        .is_some_and(|suffix| suffix.starts_with(" ("))
}

/// Matches the short cpal 0.17 device description with cpal 0.16's legacy
/// `"<name> (<driver>)"` representation in either direction.
pub fn device_names_match(first_name: &str, second_name: &str) -> bool {
    first_name == second_name
        || has_legacy_driver_suffix(first_name, second_name)
        || has_legacy_driver_suffix(second_name, first_name)
}

/// Picks the most specific name available from a cpal device description.
///
/// On Windows, cpal's WASAPI backend sets `description.name()` from the
/// endpoint's `DEVPKEY_Device_DeviceDesc` property, which is the generic
/// driver-class label (e.g. "Microphone") shared by every input device using
/// the same driver. When the more specific `DEVPKEY_Device_FriendlyName`
/// (e.g. "Microphone (Realtek(R) Audio)") differs, cpal stashes it as the
/// first `extended()` line instead of using it for `name()`. Prefer that
/// line so devices sharing a driver don't all enumerate under one identical
/// generic label.
fn preferred_device_name(description: &cpal::DeviceDescription) -> String {
    description
        .extended()
        .first()
        .cloned()
        .unwrap_or_else(|| description.name().to_string())
}

fn device_name(device: &cpal::Device) -> Result<String, cpal::DeviceNameError> {
    device
        .description()
        .map(|description| preferred_device_name(&description))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::DeviceDescriptionBuilder;

    #[test]
    fn prefers_specific_friendly_name_over_generic_device_class_description() {
        let description = DeviceDescriptionBuilder::new("Microphone")
            .add_extended_line("Microphone (Realtek(R) Audio)")
            .build();

        assert_eq!(
            preferred_device_name(&description),
            "Microphone (Realtek(R) Audio)"
        );
    }

    #[test]
    fn falls_back_to_generic_name_when_no_extended_line_present() {
        let description = DeviceDescriptionBuilder::new("USB Audio Device").build();

        assert_eq!(preferred_device_name(&description), "USB Audio Device");
    }
}

pub fn list_input_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device_name(&device).ok());

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.input_devices()?.enumerate() {
        let stable_id = device.id().ok().map(|id| id.to_string());
        let name = device_name(&device).unwrap_or_else(|_| "Unknown".into());

        let is_default = Some(name.clone()) == default_name;

        out.push(CpalDeviceInfo {
            index: index.to_string(),
            name,
            stable_id,
            is_default,
            device,
        });
    }

    Ok(out)
}

pub fn list_output_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host
        .default_output_device()
        .and_then(|device| device_name(&device).ok());

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.output_devices()?.enumerate() {
        let stable_id = device.id().ok().map(|id| id.to_string());
        let name = device_name(&device).unwrap_or_else(|_| "Unknown".into());

        let is_default = Some(name.clone()) == default_name;

        out.push(CpalDeviceInfo {
            index: index.to_string(),
            name,
            stable_id,
            is_default,
            device,
        });
    }

    Ok(out)
}

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

fn device_name(device: &cpal::Device) -> Result<String, cpal::DeviceNameError> {
    device
        .description()
        .map(|description| description.name().to_string())
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

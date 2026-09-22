use cidre::{cf, core_audio as ca, io};

use crate::{AudioDevice, AudioDeviceBackend, AudioDirection, DeviceId, Error, TransportType};

pub struct MacOSBackend;

impl MacOSBackend {
    fn ca_transport_to_transport_type(transport: ca::DeviceTransportType) -> TransportType {
        if transport == ca::DeviceTransportType::BUILT_IN {
            TransportType::BuiltIn
        } else if transport == ca::DeviceTransportType::USB {
            TransportType::Usb
        } else if transport == ca::DeviceTransportType::BLUETOOTH
            || transport == ca::DeviceTransportType::BLUETOOTH_LE
        {
            TransportType::Bluetooth
        } else if transport == ca::DeviceTransportType::HDMI
            || transport == ca::DeviceTransportType::DISPLAY_PORT
        {
            TransportType::Hdmi
        } else if transport == ca::DeviceTransportType::PCI {
            TransportType::Pci
        } else if transport == ca::DeviceTransportType::VIRTUAL
            || transport == ca::DeviceTransportType::AGGREGATE
        {
            TransportType::Virtual
        } else {
            TransportType::Unknown
        }
    }

    fn has_streams(device: &ca::Device, scope: ca::PropScope) -> bool {
        let addr = ca::PropSelector::DEVICE_STREAMS.addr(scope, ca::PropElement::MAIN);
        device
            .prop_size(&addr)
            .map(|size| size > 0)
            .unwrap_or(false)
    }

    fn create_audio_device(
        device: &ca::Device,
        direction: AudioDirection,
        default_device_id: Option<u32>,
    ) -> Option<AudioDevice> {
        let scope = match direction {
            AudioDirection::Input => ca::PropScope::INPUT,
            AudioDirection::Output => ca::PropScope::OUTPUT,
        };

        if !Self::has_streams(device, scope) {
            return None;
        }

        let uid = device.uid().ok()?;
        let name = device.name().ok()?;
        let transport_type = device
            .transport_type()
            .map(Self::ca_transport_to_transport_type)
            .unwrap_or(TransportType::Unknown);

        let is_default = default_device_id
            .map(|id| device.0.0 == id)
            .unwrap_or(false);

        let mut audio_device = AudioDevice {
            id: DeviceId::new(uid.to_string()),
            name: name.to_string(),
            direction,
            transport_type,
            is_default,
            volume: None,
            is_muted: None,
        };

        if direction == AudioDirection::Output {
            let volume_addr = ca::PropSelector::DEVICE_VOLUME_SCALAR
                .addr(ca::PropScope::OUTPUT, ca::PropElement::MAIN);
            if let Ok(volume) = device.prop::<f32>(&volume_addr) {
                audio_device.volume = Some(volume);
            }

            let mute_addr = ca::PropSelector::DEVICE_PROCESS_MUTE
                .addr(ca::PropScope::OUTPUT, ca::PropElement::MAIN);
            if let Ok(mute_value) = device.prop::<u32>(&mute_addr) {
                audio_device.is_muted = Some(mute_value != 0);
            }
        }

        Some(audio_device)
    }

    fn is_headphone_from_device(device: Option<ca::Device>) -> Option<bool> {
        let device = device?;
        let streams = device.streams().ok()?;

        let detected = streams.iter().any(|s| {
            s.terminal_type().ok().is_some_and(|term_type| {
                term_type == ca::StreamTerminalType::HEADPHONES
                    || term_type == ca::StreamTerminalType::HEADSET_MIC
                    || term_type.0 == io::audio::output_term::HEADPHONES
                    || term_type.0 == io::audio::output_term::HEAD_MOUNTED_DISPLAY_AUDIO
            })
        });

        if detected { Some(true) } else { None }
    }

    fn is_external_from_device(device: Option<ca::Device>) -> bool {
        device
            .and_then(|d| d.transport_type().ok())
            .map(|t| t != ca::DeviceTransportType::BUILT_IN)
            .unwrap_or(false)
    }
}

/// Whether a device counts as headphones from transport type and product name alone -- used
/// only once CoreAudio's own stream terminal type (checked by the caller first) did not already
/// say so.
///
/// A Bluetooth (or USB) transport says nothing about speaker vs. headphones by itself -- a
/// soundbar, boombox, or car stereo is Bluetooth too. So this stays a narrow allowlist of
/// well-known headphone/headset/earbud product-name vocabulary rather than a blanket "this
/// transport implies headphones" rule: where the name doesn't say headphones, the answer is
/// "not a headphone" (fed into mic-isolation as "not isolated"). That is the safe direction --
/// a missed automatic voiceprint exemplar costs nothing, a wrong one poisons every later
/// speaker match built from it.
///
/// Measured 2026-09-09 on the operator's own Mac against his actual Bluetooth headphones (Sony MDR-1000X,
/// the only real headphone this app has been tested against): the CoreAudio stream terminal-type
/// check above already reports them correctly on its own (`is_headphone_from_device` ==
/// `Some(true)`), so this name fallback never had to fire for the one real device that matters
/// here. It exists only as a safety net for headphones whose driver doesn't declare a
/// `StreamTerminalType`, not as the primary signal -- and it is not a substitute for testing
/// against a real Bluetooth *speaker*, which nobody on this project owns to measure against.
fn transport_and_name_suggest_headphone(transport: TransportType, name: &str) -> bool {
    match transport {
        TransportType::Bluetooth | TransportType::Usb => {
            let name_lower = name.to_lowercase();
            name_lower.contains("headphone")
                || name_lower.contains("headset")
                || name_lower.contains("airpod")
                || name_lower.contains("earbud")
        }
        TransportType::BuiltIn => name.to_lowercase().contains("headphone"),
        _ => false,
    }
}

const TAP_DEVICE_NAME: &str = "anarlog-audio-tap";

impl AudioDeviceBackend for MacOSBackend {
    fn list_devices(&self) -> Result<Vec<AudioDevice>, Error> {
        let ca_devices =
            ca::System::devices().map_err(|e| Error::EnumerationFailed(format!("{:?}", e)))?;

        let default_input_id = ca::System::default_input_device().ok().map(|d| d.0.0);
        let default_output_id = ca::System::default_output_device().ok().map(|d| d.0.0);

        let mut devices = Vec::new();

        for ca_device in ca_devices {
            if let Ok(name) = ca_device.name() {
                if name.to_string().contains(TAP_DEVICE_NAME) {
                    continue;
                }
            }

            if let Some(input_device) =
                Self::create_audio_device(&ca_device, AudioDirection::Input, default_input_id)
            {
                devices.push(input_device);
            }

            if let Some(output_device) =
                Self::create_audio_device(&ca_device, AudioDirection::Output, default_output_id)
            {
                devices.push(output_device);
            }
        }

        Ok(devices)
    }

    fn get_default_input_device(&self) -> Result<Option<AudioDevice>, Error> {
        let ca_device = match ca::System::default_input_device() {
            Ok(d) => d,
            Err(_) => return Ok(None),
        };

        if ca_device.is_unknown() {
            return Ok(None);
        }

        Ok(Self::create_audio_device(
            &ca_device,
            AudioDirection::Input,
            Some(ca_device.0.0),
        ))
    }

    fn get_default_output_device(&self) -> Result<Option<AudioDevice>, Error> {
        let ca_device = match ca::System::default_output_device() {
            Ok(d) => d,
            Err(_) => return Ok(None),
        };

        if ca_device.is_unknown() {
            return Ok(None);
        }

        Ok(Self::create_audio_device(
            &ca_device,
            AudioDirection::Output,
            Some(ca_device.0.0),
        ))
    }

    fn set_default_input_device(&self, device_id: &DeviceId) -> Result<(), Error> {
        let uid = cf::String::from_str(device_id.as_str());
        let ca_device = ca::Device::with_uid(&uid)
            .map_err(|e| Error::DeviceNotFound(format!("{}: {:?}", device_id, e)))?;

        if ca_device.is_unknown() {
            return Err(Error::DeviceNotFound(device_id.to_string()));
        }

        ca::System::OBJ
            .set_prop(
                &ca::PropSelector::HW_DEFAULT_INPUT_DEVICE.global_addr(),
                &ca_device.0,
            )
            .map_err(|e| Error::SetDefaultFailed(format!("{:?}", e)))?;

        Ok(())
    }

    fn set_default_output_device(&self, device_id: &DeviceId) -> Result<(), Error> {
        let uid = cf::String::from_str(device_id.as_str());
        let ca_device = ca::Device::with_uid(&uid)
            .map_err(|e| Error::DeviceNotFound(format!("{}: {:?}", device_id, e)))?;

        if ca_device.is_unknown() {
            return Err(Error::DeviceNotFound(device_id.to_string()));
        }

        ca::System::OBJ
            .set_prop(
                &ca::PropSelector::HW_DEFAULT_OUTPUT_DEVICE.global_addr(),
                &ca_device.0,
            )
            .map_err(|e| Error::SetDefaultFailed(format!("{:?}", e)))?;

        Ok(())
    }

    fn is_headphone(&self, device: &AudioDevice) -> bool {
        if device.direction != AudioDirection::Output {
            return false;
        }

        let uid = cf::String::from_str(device.id.as_str());
        if let Ok(ca_device) = ca::Device::with_uid(&uid) {
            if let Some(true) = Self::is_headphone_from_device(Some(ca_device)) {
                return true;
            }
        }

        transport_and_name_suggest_headphone(device.transport_type, &device.name)
    }

    fn get_device_volume(&self, device_id: &DeviceId) -> Result<f32, Error> {
        let uid = cf::String::from_str(device_id.as_str());
        let ca_device = ca::Device::with_uid(&uid)
            .map_err(|e| Error::DeviceNotFound(format!("{}: {:?}", device_id, e)))?;

        if ca_device.is_unknown() {
            return Err(Error::DeviceNotFound(device_id.to_string()));
        }

        let addr = ca::PropSelector::DEVICE_VOLUME_SCALAR
            .addr(ca::PropScope::OUTPUT, ca::PropElement::MAIN);

        ca_device
            .prop(&addr)
            .map_err(|e| Error::AudioSystemError(format!("Failed to get volume: {:?}", e)))
    }

    fn set_device_volume(&self, device_id: &DeviceId, volume: f32) -> Result<(), Error> {
        let volume = volume.clamp(0.0, 1.0);

        let uid = cf::String::from_str(device_id.as_str());
        let ca_device = ca::Device::with_uid(&uid)
            .map_err(|e| Error::DeviceNotFound(format!("{}: {:?}", device_id, e)))?;

        if ca_device.is_unknown() {
            return Err(Error::DeviceNotFound(device_id.to_string()));
        }

        let addr = ca::PropSelector::DEVICE_VOLUME_SCALAR
            .addr(ca::PropScope::OUTPUT, ca::PropElement::MAIN);

        ca_device
            .set_prop(&addr, &volume)
            .map_err(|e| Error::AudioSystemError(format!("Failed to set volume: {:?}", e)))
    }

    fn is_device_muted(&self, device_id: &DeviceId) -> Result<bool, Error> {
        let uid = cf::String::from_str(device_id.as_str());
        let ca_device = ca::Device::with_uid(&uid)
            .map_err(|e| Error::DeviceNotFound(format!("{}: {:?}", device_id, e)))?;

        if ca_device.is_unknown() {
            return Err(Error::DeviceNotFound(device_id.to_string()));
        }

        let addr = ca::PropSelector::DEVICE_PROCESS_MUTE
            .addr(ca::PropScope::OUTPUT, ca::PropElement::MAIN);

        let mute_value: u32 = ca_device
            .prop(&addr)
            .map_err(|e| Error::AudioSystemError(format!("Failed to get mute state: {:?}", e)))?;

        Ok(mute_value != 0)
    }

    fn set_device_mute(&self, device_id: &DeviceId, muted: bool) -> Result<(), Error> {
        let uid = cf::String::from_str(device_id.as_str());
        let ca_device = ca::Device::with_uid(&uid)
            .map_err(|e| Error::DeviceNotFound(format!("{}: {:?}", device_id, e)))?;

        if ca_device.is_unknown() {
            return Err(Error::DeviceNotFound(device_id.to_string()));
        }

        let addr = ca::PropSelector::DEVICE_PROCESS_MUTE
            .addr(ca::PropScope::OUTPUT, ca::PropElement::MAIN);

        let mute_value: u32 = if muted { 1 } else { 0 };

        ca_device
            .set_prop(&addr, &mute_value)
            .map_err(|e| Error::AudioSystemError(format!("Failed to set mute state: {:?}", e)))
    }
}

pub fn is_headphone_from_default_output_device() -> Option<bool> {
    let device = ca::System::default_output_device().ok();
    MacOSBackend::is_headphone_from_device(device)
}

pub fn is_headphone_from_default_input_device() -> Option<bool> {
    let device = ca::System::default_input_device().ok();
    MacOSBackend::is_headphone_from_device(device)
}

pub fn is_default_input_external() -> bool {
    let device = ca::System::default_input_device().ok();
    MacOSBackend::is_external_from_device(device)
}

pub fn is_default_output_external() -> bool {
    let device = ca::System::default_output_device().ok();
    MacOSBackend::is_external_from_device(device)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_speaker_by_name_is_not_a_headphone() {
        // Real product names of Bluetooth *speakers* -- none of them are headphones, and none
        // contain the headphone/headset/airpod/earbud vocabulary the name fallback allowlists.
        for name in ["JBL Charge 5", "Sonos Move", "Bose SoundLink Speaker", "UE Boom 3"] {
            assert!(
                !transport_and_name_suggest_headphone(TransportType::Bluetooth, name),
                "{name} must not be classified as headphones"
            );
        }
    }

    #[test]
    fn bluetooth_headphone_by_name_is_still_recognized() {
        for name in ["Mads' AirPods Pro", "Sony WH Headset", "Bose Headphones"] {
            assert!(
                transport_and_name_suggest_headphone(TransportType::Bluetooth, name),
                "{name} should be classified as headphones"
            );
        }
    }

    #[test]
    fn unnamed_bluetooth_device_without_headphone_terminal_type_fails_closed() {
        // The exact real case measured 2026-09-09: a Bluetooth headphone whose product name
        // (like the operator's "MDR-1000X") carries none of the allowlisted keywords. In production
        // `is_headphone()` still classifies this correctly because the CoreAudio stream
        // terminal-type check runs first and catches it -- this function is only the fallback
        // for when that check comes back empty, and for an unlabeled Bluetooth device the safe
        // answer is "not a headphone", not "assume yes".
        assert!(!transport_and_name_suggest_headphone(
            TransportType::Bluetooth,
            "MDR-1000X"
        ));
    }

    #[test]
    fn non_bluetooth_non_usb_non_builtin_transports_are_never_headphones_by_name_alone() {
        for transport in [
            TransportType::Hdmi,
            TransportType::Pci,
            TransportType::Virtual,
            TransportType::Unknown,
        ] {
            assert!(!transport_and_name_suggest_headphone(
                transport,
                "Anything Headphones"
            ));
        }
    }

    #[test]
    fn test_list_devices() {
        let backend = MacOSBackend;
        match backend.list_devices() {
            Ok(devices) => {
                println!("Found {} devices:", devices.len());
                for device in &devices {
                    println!(
                        "  - {} ({:?}, {:?}, uid={}, default={})",
                        device.name,
                        device.direction,
                        device.transport_type,
                        device.id.0,
                        device.is_default
                    );
                }
            }
            Err(e) => {
                println!("Error listing devices: {}", e);
            }
        }
    }

    #[test]
    fn test_get_default_devices() {
        let backend = MacOSBackend;

        match backend.get_default_input_device() {
            Ok(Some(device)) => {
                println!("Default input: {} ({})", device.name, device.id.0);
            }
            Ok(None) => {
                println!("No default input device");
            }
            Err(e) => {
                println!("Error getting default input: {}", e);
            }
        }

        match backend.get_default_output_device() {
            Ok(Some(device)) => {
                println!("Default output: {} ({})", device.name, device.id.0);
                println!("Is headphone: {}", backend.is_headphone(&device));
            }
            Ok(None) => {
                println!("No default output device");
            }
            Err(e) => {
                println!("Error getting default output: {}", e);
            }
        }
    }

    #[test]
    fn test_headphone_detection() {
        println!(
            "is_headphone_from_default_output_device={:?}",
            is_headphone_from_default_output_device()
        );
        println!(
            "is_headphone_from_default_input_device={:?}",
            is_headphone_from_default_input_device()
        );
        println!("is_default_input_external={}", is_default_input_external());
        println!(
            "is_default_output_external={}",
            is_default_output_external()
        );
    }
}

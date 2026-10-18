//! Where a track's audio comes from: the default input device, or a process tap inside a
//! private aggregate device, and the Core Audio property reads they need.

use std::ffi::CStr;
use std::ptr::{self, NonNull};

use anyhow::bail;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_audio::{
    AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
    AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap,
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress, CATapDescription,
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioDevicePropertyNominalSampleRate,
    kAudioHardwarePropertyDefaultInputDevice, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioObjectUnknown,
    kAudioSubTapUIDKey,
};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSNumber, NSString, NSUUID};

use crate::Track;
use crate::macos::file::check;

/// Where a track's audio comes from.
pub(super) struct Source {
    pub(super) device: AudioObjectID,
    pub(super) sample_rate: f64,
    /// For the system track: the tap and its aggregate device, destroyed with the capture.
    tap: Option<(AudioObjectID, AudioObjectID)>,
}

impl Source {
    pub(super) fn open(track: Track) -> anyhow::Result<Self> {
        let (device, tap) = match track {
            Track::Mic => {
                let device: AudioObjectID = property(
                    kAudioObjectSystemObject as AudioObjectID,
                    kAudioHardwarePropertyDefaultInputDevice,
                )?;
                if device == kAudioObjectUnknown {
                    bail!("no input device");
                }
                (device, None)
            }
            Track::System => {
                let (tap, aggregate) = system_tap()?;
                (aggregate, Some((tap, aggregate)))
            }
        };
        // Built first, so an error below still destroys the tap.
        let mut source = Self {
            device,
            sample_rate: 0.0,
            tap,
        };
        source.sample_rate = property(device, kAudioDevicePropertyNominalSampleRate)?;
        if source.sample_rate <= 0.0 {
            bail!("the device reports no sample rate");
        }
        Ok(source)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        if let Some((tap, aggregate)) = self.tap.take() {
            // SAFETY: both were created by `system_tap` and are destroyed only here.
            unsafe {
                AudioHardwareDestroyAggregateDevice(aggregate);
                AudioHardwareDestroyProcessTap(tap);
            }
        }
    }
}

/// A mono tap of every process's output except Crumble's, inside a private aggregate device
/// whose input is the tap. Returns the tap and the device.
fn system_tap() -> anyhow::Result<(AudioObjectID, AudioObjectID)> {
    // SAFETY: plain Objective-C calls on objects created here; the arrays hold the stated types.
    let description = unsafe {
        let description = CATapDescription::initMonoGlobalTapButExcludeProcesses(
            CATapDescription::alloc(),
            &NSArray::new(),
        );
        description.setPrivate(true);
        description.setName(&NSString::from_str("Crumble Recording"));
        // Leaves out Crumble's own sound, such as a Recording being played back.
        if let Some(bundle) = NSBundle::mainBundle().bundleIdentifier() {
            description.setBundleIDs(&NSArray::from_retained_slice(&[bundle]));
        }
        description
    };
    let mut tap = kAudioObjectUnknown;
    // SAFETY: a valid description, and `tap` receives the id.
    check(
        unsafe { AudioHardwareCreateProcessTap(Some(&description), &mut tap) },
        "create the system audio tap",
    )?;
    // SAFETY: a getter on the description made above.
    let tap_uid = unsafe { description.UUID().UUIDString() };
    let sub_tap =
        NSDictionary::from_retained_objects(&[&*key(kAudioSubTapUIDKey)], &[any(tap_uid)]);
    let keys = [
        key(kAudioAggregateDeviceUIDKey),
        key(kAudioAggregateDeviceNameKey),
        key(kAudioAggregateDeviceIsPrivateKey),
        key(kAudioAggregateDeviceTapAutoStartKey),
        key(kAudioAggregateDeviceTapListKey),
    ];
    let values: [Retained<AnyObject>; 5] = [
        any(NSUUID::new().UUIDString()),
        any(NSString::from_str("Crumble Recording")),
        any(NSNumber::new_bool(true)),
        any(NSNumber::new_bool(true)),
        any(NSArray::from_retained_slice(&[sub_tap])),
    ];
    let keys: Vec<&NSString> = keys.iter().map(|key| &**key).collect();
    let description = NSDictionary::from_retained_objects(&keys, &values);
    // SAFETY: NSDictionary is toll-free bridged to CFDictionary.
    let description: &CFDictionary =
        unsafe { &*ptr::from_ref(&*description).cast::<CFDictionary>() };
    let mut aggregate = kAudioObjectUnknown;
    // SAFETY: a valid dictionary, and `aggregate` receives the id.
    let status =
        unsafe { AudioHardwareCreateAggregateDevice(description, NonNull::from(&mut aggregate)) };
    if let Err(error) = check(status, "create the system audio device") {
        // SAFETY: the tap was created above and is not used again.
        unsafe { AudioHardwareDestroyProcessTap(tap) };
        return Err(error);
    }
    Ok((tap, aggregate))
}

fn key(key: &CStr) -> Retained<NSString> {
    NSString::from_str(&key.to_string_lossy())
}

fn any<T: objc2::Message>(object: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an AnyObject.
    unsafe { Retained::cast_unchecked(object) }
}

/// A global property of `object`, of type `T`.
fn property<T: Default>(object: AudioObjectID, selector: u32) -> anyhow::Result<T> {
    let address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // SAFETY: `value` holds `size` bytes of the property's type.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    check(status, "read a device property")?;
    Ok(value)
}

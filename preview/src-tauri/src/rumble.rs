#[cfg(windows)]
pub fn pulse(strength_percent: u8, duration_ms: u64) {
    std::thread::spawn(move || unsafe {
        use windows_sys::Win32::UI::Input::XboxController::{XINPUT_VIBRATION, XInputSetState};
        let strength = (u32::from(u16::MAX) * u32::from(strength_percent.min(100)) / 100) as u16;
        let vibration = XINPUT_VIBRATION {
            wLeftMotorSpeed: strength,
            wRightMotorSpeed: strength,
        };
        for index in 0..4 {
            XInputSetState(index, &vibration);
        }
        std::thread::sleep(std::time::Duration::from_millis(duration_ms.min(5_000)));
        let stopped = XINPUT_VIBRATION {
            wLeftMotorSpeed: 0,
            wRightMotorSpeed: 0,
        };
        for index in 0..4 {
            XInputSetState(index, &stopped);
        }
    });
}

#[cfg(not(windows))]
pub fn pulse(_strength_percent: u8, _duration_ms: u64) {}

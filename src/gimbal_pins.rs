use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::gpio::AnyOutputPin;
use esp_idf_svc::hal::gpio::Input;
use esp_idf_svc::hal::gpio::Output;
use esp_idf_svc::hal::gpio::PinDriver;

pub struct GimbalPins {
    pub pan_dir: OutPin,
    pub pan_step: OutPin,
    pub tilt_dir: OutPin,
    pub tilt_step: OutPin,
    pub pan_endstop: InPin,
    pub tilt_endstop: InPin,
}

pub struct GimbalBuilder;
pub struct PanStep(OutPin);
pub struct TiltDir(OutPin, OutPin);
pub struct TiltStep(OutPin, OutPin, OutPin);
pub struct PanEndStop(OutPin, OutPin, OutPin, OutPin);
pub struct TiltEndStop(OutPin, OutPin, OutPin, OutPin, InPin);

impl GimbalBuilder {
    pub fn pan_dir(pin: OutPin) -> PanStep {
        PanStep(pin)
    }
}
impl PanStep {
    pub fn pan_step(self, pin: OutPin) -> TiltDir {
        TiltDir(self.0, pin)
    }
}
impl TiltDir {
    pub fn tilt_dir(self, pin: OutPin) -> TiltStep {
        TiltStep(self.0, self.1, pin)
    }
}
impl TiltStep {
    pub fn tilt_step(self, pin: OutPin) -> PanEndStop {
        PanEndStop(self.0, self.1, self.2, pin)
    }
}
impl PanEndStop {
    pub fn pan_endstop(self, pin: InPin) -> TiltEndStop {
        TiltEndStop(self.0, self.1, self.2, self.3, pin)
    }
}
impl TiltEndStop {
    pub fn tilt_endstop(self, pin: InPin) -> GimbalPins {
        GimbalPins {
            pan_dir: self.0,
            pan_step: self.1,
            tilt_dir: self.2,
            tilt_step: self.3,
            pan_endstop: self.4,
            tilt_endstop: pin,
        }
    }
}

pub struct OutPin {
    pub pd: PinDriver<'static, AnyOutputPin, Output>,
}

impl OutPin {
    pub fn high(&mut self) {
        self.pd.set_high().expect("failed to set high");
    }
    pub fn low(&mut self) {
        self.pd.set_low().expect("failed to set low");
    }
}

impl From<AnyOutputPin> for OutPin {
    fn from(value: AnyOutputPin) -> Self {
        OutPin {
            pd: PinDriver::output(value).expect("failed to get pin driver"),
        }
    }
}

pub struct InPin {
    pub pd: PinDriver<'static, AnyIOPin, Input>,
}

impl From<AnyIOPin> for InPin {
    fn from(value: AnyIOPin) -> Self {
        InPin {
            pd: PinDriver::input(value).expect("failed to get pin driver"),
        }
    }
}

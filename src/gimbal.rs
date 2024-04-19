use serde::Serialize;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::anyhow;
use libm::floorf;

use derive_more::Display;

use log::info;

use esp_idf_svc::hal::delay::Delay;
use esp_idf_svc::hal::gpio::Level;
use esp_idf_svc::hal::gpio::Pull;
use esp_idf_svc::hal::task::notification::Notification;

use crate::gcode::Gcode;
use crate::gimbal_pins::GimbalPins;
use crate::motor::steps_per_degree;
use crate::mv::Move;

#[derive(Copy, Clone, Debug, Display)]
pub enum Axis {
    #[display(fmt = "Pan")]
    Pan,
    #[display(fmt = "Tilt")]
    Tilt,
}

#[derive(Serialize)]
pub struct Gimbal {
    #[serde(skip)]
    pub pins: GimbalPins,
    pub pos_steps: (u32, u32),
    pan_teeth: u16,
    tilt_teeth: u16,
    pan_drive_teeth: u16,
    tilt_drive_teeth: u16,
    pan_velocity: f32,
    tilt_velocity: f32,
    is_home_referenced: bool,
    is_homing: bool,
    pub last_error_message: Option<String>,
}

impl Gimbal {
    pub fn new(
        mut pins: GimbalPins,
        pan_teeth: u16,
        pan_drive_teeth: u16,
        tilt_teeth: u16,
        tilt_drive_teeth: u16,
        pan_velocity: f32,
        tilt_velocity: f32,
    ) -> Self {
        pins.pan_endstop
            .pd
            .set_pull(Pull::Up)
            .expect("pullup failed");
        pins.tilt_endstop
            .pd
            .set_pull(Pull::Up)
            .expect("pullup failed");
        Self {
            pins,
            pos_steps: (0, 0),
            pan_teeth,
            tilt_teeth,
            pan_drive_teeth,
            tilt_drive_teeth,
            pan_velocity,
            tilt_velocity,
            is_homing: false,
            // @todo NO NO NO NO
            is_home_referenced: true,
            last_error_message: None,
        }
    }

    fn steps_per_degree_pan(&self) -> f32 {
        steps_per_degree(self.pan_drive_teeth, self.pan_teeth)
    }

    fn steps_per_degree_tilt(&self) -> f32 {
        steps_per_degree(self.tilt_drive_teeth, self.tilt_teeth)
    }

    pub fn fire() {
        todo!()
    }

    fn try_home(&mut self) -> anyhow::Result<()> {
        self.home_axis(&Axis::Pan)?;
        self.home_axis(&Axis::Tilt)?;
        Ok(())
    }

    pub fn process_gcode(&mut self, gcode: Gcode) -> anyhow::Result<()> {
        info!("processing gcode [START]: {gcode:?}");
        match gcode {
            Gcode::G1Move(opan, otilt) => {
                if !self.is_home_referenced && !self.is_homing {
                    return Err(anyhow!("gimbal not homed"));
                }
                let pan = opan.unwrap_or(0.);
                let tilt = otilt.unwrap_or(0.);

                self.moov(Move {
                    axis: Axis::Pan,
                    degrees: pan,
                });

                self.moov(Move {
                    axis: Axis::Tilt,
                    degrees: tilt,
                });
            }
            Gcode::G28Home => {
                self.is_homing = true;
                let res = self.try_home();
                self.is_homing = false;
                self.is_home_referenced = res.is_ok();
                res?;
            }
            Gcode::G90SetAbsolute => todo!(),
            Gcode::G91SetRelative => todo!(),
            Gcode::M1SetVelocity(opan, otilt) => {
                let pan = opan.unwrap_or(self.pan_velocity);
                let tilt = otilt.unwrap_or(self.tilt_velocity);
                self.pan_velocity = pan;
                self.tilt_velocity = tilt;
            }
        };

        Ok(())
    }

    fn is_home(&self, axis: &Axis) -> bool {
        match axis {
            Axis::Pan => self.is_pan_home(),
            Axis::Tilt => self.is_tilt_home(),
        }
    }

    fn home_axis(&mut self, axis: &Axis) -> anyhow::Result<()> {
        let mut iter_deg = 1.;
        let mut max_iter = (360. / iter_deg) as u32;

        if self.is_home(axis) {
            return Ok(());
        }

        // coarse
        for _ in 1..=max_iter {
            self.moov(Move {
                axis: *axis,
                degrees: -iter_deg,
            });
            if self.is_home(axis) {
                break;
            }
        }

        if !self.is_home(axis) {
            return Err(anyhow!("failed to home {axis} (coarse pass)"));
        }

        // backoff to prep for fine approach
        let backoff_deg = 4.;
        self.moov(Move {
            axis: *axis,
            degrees: backoff_deg * iter_deg,
        });

        // refine back in incr
        let finer_by = 5.;
        iter_deg /= finer_by;
        max_iter = ((backoff_deg * finer_by) * 1.1).floor() as u32;

        // fine
        let last_v = match axis {
            Axis::Pan => self.pan_velocity,
            Axis::Tilt => self.tilt_velocity,
        };

        // slow us down mate
        match axis {
            Axis::Pan => self.pan_velocity /= 5.,
            Axis::Tilt => self.tilt_velocity /= 5.,
        };

        for _ in 1..=max_iter {
            self.moov(Move {
                axis: *axis,
                degrees: -iter_deg,
            });
            if self.is_home(axis) {
                break;
            }
        }

        if !self.is_home(axis) {
            return Err(anyhow!("failed to home {axis} (fine pass)"));
        }

        // ...annnnnnnnd speed us back up
        match axis {
            Axis::Pan => self.pan_velocity = last_v,
            Axis::Tilt => self.tilt_velocity = last_v,
        };

        Ok(())
    }

    fn is_pan_home(&self) -> bool {
        self.pins.pan_endstop.pd.get_level() == Level::Low
    }

    fn is_tilt_home(&self) -> bool {
        self.pins.pan_endstop.pd.get_level() == Level::Low
    }

    fn moov(&mut self, mv: Move) {
        let Move { axis, degrees } = mv;
        let is_fwd = degrees > 0.;

        // calculate how many steps to take
        let steps_per_degree = match &axis {
            Axis::Pan => self.steps_per_degree_pan(),
            Axis::Tilt => self.steps_per_degree_tilt(),
        };

        let velocity = match &axis {
            Axis::Pan => self.pan_velocity,
            Axis::Tilt => self.tilt_velocity,
        };

        let num_steps = floorf(degrees.abs() * steps_per_degree) as u32;

        let steps_per_second = floorf(
            /* deg / s */ velocity * /* step / deg */ steps_per_degree,
        ) as u32;

        let step_pin = match axis {
            Axis::Pan => &mut self.pins.pan_step,
            Axis::Tilt => &mut self.pins.tilt_step,
        };

        let dir_pin = match axis {
            Axis::Pan => &mut self.pins.pan_dir,
            Axis::Tilt => &mut self.pins.tilt_dir,
        };

        // setup direction
        match is_fwd {
            true => dir_pin.high(),
            false => dir_pin.low(),
        };

        let steps_per_microsecond = (steps_per_second as f32) / (1_000_000.);
        let microseconds_per_step = 1. / steps_per_microsecond;
        let delay_micros = libm::floorf(microseconds_per_step / 2.) as u32;

        let seconds_to_move = (num_steps as f32) / steps_per_second as f32;

        info!("move // axis: {axis}, degrees: {degrees}, steps: {num_steps}, delay_micros: {delay_micros}, seconds_to_move: {seconds_to_move:.2}");

        for _i in 0..num_steps {
            step_pin.high();
            Delay::new_default().delay_us(delay_micros);
            step_pin.low();
            Delay::new_default().delay_us(delay_micros);
        }
        self.pos_steps = match &axis {
            Axis::Pan => (self.pos_steps.0 + num_steps, self.pos_steps.1),
            Axis::Tilt => (self.pos_steps.0, self.pos_steps.1 + num_steps),
        };
    }

    pub fn home(&mut self) -> anyhow::Result<()> {
        let max_degrees = 360.;
        let max_pan_steps = (max_degrees * self.steps_per_degree_pan()) as u32;
        self.home_pin(Axis::Pan, max_pan_steps)?;

        let _max_tilt_steps = (max_degrees * self.steps_per_degree_tilt()) as u32;
        Ok(())
    }

    // @todo NOT DONE
    fn home_pin(&mut self, axis: Axis, _max_steps: u32) -> anyhow::Result<()> {
        let endstop_pin = match axis {
            Axis::Pan => &mut self.pins.pan_endstop,
            Axis::Tilt => &mut self.pins.tilt_endstop,
        };
        let _ = endstop_pin.pd.set_pull(Pull::Up);
        let _ = endstop_pin
            .pd
            .set_interrupt_type(esp_idf_svc::hal::gpio::InterruptType::LowLevel);
        let notification = Notification::new();
        let notifier = notification.notifier();

        let homed = Arc::new(Mutex::new(false));
        let homed_waiter = homed.clone();
        let homed_notify = homed.clone();

        unsafe {
            endstop_pin.pd.subscribe(move || {
                info!("endstop hit {axis}");
                *homed_notify.lock().unwrap() = true;
                notifier.notify_and_yield(NonZeroU32::new(1).unwrap());
            })?;
        }

        endstop_pin.pd.enable_interrupt()?;

        let mut halted = false;
        while !halted {
            // @todo NOT DONE
            notification.wait(esp_idf_svc::hal::delay::NON_BLOCK);
            Delay::new_default().delay_ms(1);
            {
                halted = *homed_waiter.lock().unwrap();
            }
        }
        Ok(())
    }
}
